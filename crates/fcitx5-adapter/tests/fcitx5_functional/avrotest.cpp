// Real-fcitx5 functional test for the AvroPhonetic addon.
//
// This is NOT a `cargo test` and is not built by `cargo build`/`cargo test`.
// It links against the real fcitx5 daemon libraries (Fcitx5Core) and fcitx5's
// upstream TestFrontend module to inject real keystrokes through a real
// installed `AvroPhonetic` addon, entirely headlessly (no X11/Wayland needed).
//
// Requires: the `fcitx5`/`fcitx5-devel` packages and the AvroPhonetic addon
// already installed (via `make build && make install`), plus
// `testfrontend.conf` made discoverable under the normal
// `/usr/share/fcitx5/addon/` directory (see .github/workflows/ci.yml).
//
// Build:
//   g++ -std=c++20 avrotest.cpp $(pkg-config --cflags --libs Fcitx5Core) \
//       -I/usr/include/Fcitx5/Module/fcitx-module/testfrontend -o avrotest
// Run:
//   ./avrotest
//
// A clean exit (0) plus the printed "avrotest finished cleanly" line means
// success. FCITX_ASSERT aborts the process (non-zero/signal exit) on any
// failure, including a pushed commit expectation that was never consumed.

#include "fcitx-utils/eventdispatcher.h"
#include "fcitx-utils/key.h"
#include "fcitx-utils/keysym.h"
#include "fcitx-utils/log.h"
#include "fcitx/addonmanager.h"
#include "fcitx/inputmethodgroup.h"
#include "fcitx/inputmethodmanager.h"
#include "fcitx/instance.h"
#include "testfrontend_public.h"

#include <string>

using namespace fcitx;

void scheduleEvent(EventDispatcher *dispatcher, Instance *instance) {
    dispatcher->schedule([instance]() {
        InputMethodGroup group("Test");
        group.setDefaultLayout("us");
        group.inputMethodList().push_back(InputMethodGroupItem("avro"));
        instance->inputMethodManager().addEmptyGroup("Test");
        instance->inputMethodManager().setGroup(group);
        instance->inputMethodManager().setCurrentGroup("Test");
    });
    dispatcher->schedule([dispatcher, instance]() {
        auto *avroAddon = instance->addonManager().addon("AvroPhonetic", true);
        FCITX_ASSERT(avroAddon) << "AvroPhonetic addon failed to load";

        auto *testfrontend = instance->addonManager().addon("testfrontend");
        FCITX_ASSERT(testfrontend) << "testfrontend addon failed to load";
        auto uuid =
            testfrontend->call<ITestFrontend::createInputContext>("avrotest");
        auto *ic = instance->inputContextManager().findByUUID(uuid);
        FCITX_ASSERT(ic) << "could not find created input context";
        FCITX_ASSERT(instance->inputMethod(ic) == "avro")
            << "expected active input method 'avro', got: "
            << instance->inputMethod(ic);

        // Ground truth: avro-core's own test suite (engine.rs) confirms
        // "bangla" -> "বাংলা". shim.cpp appends a literal space after
        // commit when the Space key triggers it.
        testfrontend->call<ITestFrontend::pushCommitExpectation>("বাংলা ");
        for (char c : std::string("bangla")) {
            testfrontend->call<ITestFrontend::keyEvent>(
                uuid, Key(std::string(1, c)), false);
        }
        testfrontend->call<ITestFrontend::keyEvent>(uuid, Key(FcitxKey_space),
                                                     false);

        dispatcher->schedule([dispatcher, instance]() {
            dispatcher->detach();
            instance->exit();
        });
    });
}

int main() {
    char arg0[] = "avrotest";
    char arg1[] = "--disable=all";
    char arg2[] = "--enable=AvroPhonetic,testfrontend";
    char *argv[] = {arg0, arg1, arg2};
    Instance instance(FCITX_ARRAY_SIZE(argv), argv);
    instance.addonManager().registerDefaultLoader(nullptr);
    EventDispatcher dispatcher;
    dispatcher.attach(&instance.eventLoop());
    scheduleEvent(&dispatcher, &instance);
    instance.exec();
    FCITX_INFO() << "avrotest finished cleanly";
    return 0;
}
