#include <fcitx/addonfactory.h>
#include <fcitx/addoninstance.h>
#include <fcitx/addonmanager.h>
#include <fcitx/candidatelist.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputcontextmanager.h>
#include <fcitx/inputcontextproperty.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/inputmethodentry.h>
#include <fcitx/inputpanel.h>
#include <fcitx/instance.h>
#include <fcitx/text.h>
#include <fcitx-utils/textformatflags.h>
#include <fcitx-utils/key.h>
#include <fcitx-utils/keysym.h>

#include <memory>
#include <string>
#include <vector>

// ── Rust FFI declarations ────────────────────────────────────────────────────

struct AvroState;

extern "C" {
    AvroState *avro_state_new(const char *grammar, const char *dict, const char *suffix);
    void       avro_state_free(AvroState *);
    char      *avro_handle_input(AvroState *, unsigned int codepoint);
    char      *avro_handle_backspace(AvroState *);
    char      *avro_commit(AvroState *);
    char      *avro_commit_suggestion(AvroState *, int index);
    int        avro_has_preedit(const AvroState *);
    char      *avro_preedit(const AvroState *);
    int        avro_suggest_count(const AvroState *);
    char      *avro_suggest_get(const AvroState *, int index);
    void       avro_str_free(char *);
    void       avro_publish_overlay_state(const AvroState *, int x, int y, int w, int h);
    void       avro_overlay_spawn();
    void       avro_overlay_stop();
}

// ── RAII wrapper for Rust-owned C strings ───────────────────────────────────

struct RustStr {
    char *ptr;
    explicit RustStr(char *p) : ptr(p) {}
    ~RustStr() { avro_str_free(ptr); }
    std::string str() const { return ptr ? std::string(ptr) : ""; }
    bool empty() const { return !ptr || ptr[0] == '\0'; }
    RustStr(const RustStr &) = delete;
    RustStr &operator=(const RustStr &) = delete;
};

// ── Per-InputContext state ───────────────────────────────────────────────────

class AvroProperty : public fcitx::InputContextProperty {
public:
    AvroProperty()
        : state_(avro_state_new(PKGDATADIR "/avrophonetic.json",
                                PKGDATADIR "/avrodict.js",
                                PKGDATADIR "/suffixdict.js")) {}
    ~AvroProperty() { avro_state_free(state_); }

    AvroState *state() { return state_; }

private:
    AvroState *state_;
};

// ── Shared panel/overlay reset ──────────────────────────────────────────────

static void publishOverlay(fcitx::InputContext *ic, AvroState *state) {
    const auto &rect = ic->cursorRect();
    avro_publish_overlay_state(state, rect.left(), rect.top(), rect.width(), rect.height());
}

// Clears fcitx5's own preedit/candidate panel and publishes the (now empty)
// state to the overlay-adapter, so a floating overlay window hides itself
// alongside fcitx5's own UI.
static void clearPanel(fcitx::InputContext *ic, AvroState *state) {
    ic->inputPanel().reset();
    ic->updatePreedit();
    ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    publishOverlay(ic, state);
}

// ── Candidate word ───────────────────────────────────────────────────────────

class AvroCandidateWord : public fcitx::CandidateWord {
public:
    AvroCandidateWord(AvroState *state, int index, std::string text)
        : state_(state), index_(index) {
        setText(fcitx::Text(std::move(text)));
    }

    void select(fcitx::InputContext *ic) const override {
        RustStr committed(avro_commit_suggestion(state_, index_));
        ic->commitString(committed.str());
        clearPanel(ic, state_);
    }

private:
    AvroState *state_;
    int index_;
};

// ── Engine ───────────────────────────────────────────────────────────────────

class AvroPhoneticEngine : public fcitx::InputMethodEngine {
public:
    explicit AvroPhoneticEngine(fcitx::AddonManager *manager) {
        manager->instance()->inputContextManager()
               .registerProperty("avroPhonetic", &factory_);
        avro_overlay_spawn();
    }

    ~AvroPhoneticEngine() override { avro_overlay_stop(); }

    std::vector<fcitx::InputMethodEntry> listInputMethods() override {
        std::vector<fcitx::InputMethodEntry> entries;
        entries.emplace_back("avro", "Avro Phonetic", "bn", "avro");
        entries.back().setLabel("অ").setIcon("input-bengali");
        return entries;
    }

    void deactivate(const fcitx::InputMethodEntry &entry,
                    fcitx::InputContextEvent &event) override {
        reset(entry, event);
    }

    void reset(const fcitx::InputMethodEntry &,
               fcitx::InputContextEvent &event) override {
        auto *ic = event.inputContext();
        auto *prop = ic->propertyFor(&factory_);
        avro_commit(prop->state()); // discard uncommitted text
        clearPanel(ic, prop->state());
    }

    void keyEvent(const fcitx::InputMethodEntry &, fcitx::KeyEvent &event) override {
        auto *ic = event.inputContext();
        auto *prop = ic->propertyFor(&factory_);
        AvroState *state = prop->state();

        if (event.isRelease()) return;

        const auto key = event.key();

        if (key.check(FcitxKey_BackSpace)) {
            if (!avro_has_preedit(state)) return;
            event.accept();
            avro_handle_backspace(state);
            updateUI(ic, state);
            return;
        }

        if (key.check(FcitxKey_Return) || key.check(FcitxKey_space)) {
            if (!avro_has_preedit(state)) return;
            event.accept();
            RustStr committed(avro_commit(state));
            std::string out = committed.str();
            if (key.check(FcitxKey_space)) out += ' ';
            ic->commitString(out);
            clearPanel(ic, state);
            return;
        }

        if (key.check(FcitxKey_Escape)) {
            if (!avro_has_preedit(state)) return;
            event.accept();
            RustStr discard(avro_commit(state));
            clearPanel(ic, state);
            return;
        }

        // Digit keys 1–5: pick from candidate list
        const auto sym = key.sym();
        if (avro_has_preedit(state) && sym >= FcitxKey_1 && sym <= FcitxKey_5) {
            int idx = static_cast<int>(sym - FcitxKey_1);
            if (idx < avro_suggest_count(state)) {
                event.accept();
                RustStr committed(avro_commit_suggestion(state, idx));
                ic->commitString(committed.str());
                clearPanel(ic, state);
                return;
            }
        }

        // Printable ASCII → feed to engine
        if (sym > 0x20 && sym < 0x7f) {
            event.accept();
            RustStr preedit(avro_handle_input(state, static_cast<unsigned int>(sym)));
            updateUI(ic, state);
            return;
        }
    }

private:
    fcitx::SimpleInputContextPropertyFactory<AvroProperty> factory_;

    void updateUI(fcitx::InputContext *ic, AvroState *state) {
        fcitx::Text preedit;
        RustStr ps(avro_preedit(state));
        preedit.append(ps.str(), fcitx::TextFormatFlag::Underline);
        preedit.setCursor(preedit.textLength());
        ic->inputPanel().setClientPreedit(preedit);

        const int n = avro_suggest_count(state);
        if (n > 0) {
            auto candList = std::make_unique<fcitx::CommonCandidateList>();
            candList->setLayoutHint(fcitx::CandidateLayoutHint::Horizontal);
            for (int i = 0; i < n; ++i) {
                RustStr s(avro_suggest_get(state, i));
                candList->append<AvroCandidateWord>(state, i, s.str());
            }
            ic->inputPanel().setCandidateList(std::move(candList));
        } else {
            ic->inputPanel().setCandidateList(nullptr);
        }

        ic->updatePreedit();
        ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
        publishOverlay(ic, state);
    }
};

// ── Addon factory ────────────────────────────────────────────────────────────

class AvroPhoneticFactory : public fcitx::AddonFactory {
public:
    fcitx::AddonInstance *create(fcitx::AddonManager *manager) override {
        return new AvroPhoneticEngine(manager);
    }
};

// Hand-expanded instead of FCITX_ADDON_FACTORY(AvroPhoneticFactory): a plain
// extern "C" symbol from this statically-linked object gets dropped from the
// cdylib's dynamic symbol table at link time (nothing in Rust code references
// it, so it's neither pulled in nor exported). Renamed and re-exported via a
// #[no_mangle] Rust fn in lib.rs, which rustc's own cdylib export list does
// include correctly.
extern "C" {
fcitx::AddonFactory *avro_fcitx_addon_factory_impl() {
    static AvroPhoneticFactory factory;
    return &factory;
}
}
