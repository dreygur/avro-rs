CARGO        ?= cargo
INSTALL      ?= install
PREFIX       ?= /usr
LIBDIR       ?= $(PREFIX)/lib64
FCITX5_ADDON ?= $(LIBDIR)/fcitx5
FCITX5_DATA  ?= $(PREFIX)/share/fcitx5

LIB_RELEASE  := target/release/libfcitx5_adapter.so
LIB_NAME     := libfcitx5-adapter.so

PKGDATADIR   := $(FCITX5_DATA)/avro

.PHONY: all build install uninstall clean

all: build

# Run as your regular user — cargo must not be invoked under sudo.
build:
	PKGDATADIR=$(PKGDATADIR) $(CARGO) build -p fcitx5-adapter --release

# Run as root (sudo make install). Requires 'make build' to have been run first.
install: $(LIB_RELEASE)
	$(INSTALL) -Dm755 $(LIB_RELEASE) $(DESTDIR)$(FCITX5_ADDON)/$(LIB_NAME)
	$(INSTALL) -Dm644 dist/addon/AvroPhonetic.conf \
		$(DESTDIR)$(FCITX5_DATA)/addon/AvroPhonetic.conf
	$(INSTALL) -Dm644 dist/inputmethod/avro.conf \
		$(DESTDIR)$(FCITX5_DATA)/inputmethod/avro.conf
	$(INSTALL) -dm755 $(DESTDIR)$(PKGDATADIR)
	$(INSTALL) -Dm644 avro.json      $(DESTDIR)$(PKGDATADIR)/avrophonetic.json
	$(INSTALL) -Dm644 avrodict.js    $(DESTDIR)$(PKGDATADIR)/avrodict.js
	$(INSTALL) -Dm644 suffixdict.js  $(DESTDIR)$(PKGDATADIR)/suffixdict.js
	@echo "Installed. Restart fcitx5 and enable 'Avro Phonetic' in fcitx5-configtool."

$(LIB_RELEASE):
	@echo "Run 'make build' as your regular user before 'sudo make install'."
	@exit 1

uninstall:
	rm -f  $(DESTDIR)$(FCITX5_ADDON)/$(LIB_NAME)
	rm -f  $(DESTDIR)$(FCITX5_DATA)/addon/AvroPhonetic.conf
	rm -f  $(DESTDIR)$(FCITX5_DATA)/inputmethod/avro.conf
	rm -rf $(DESTDIR)$(PKGDATADIR)

clean:
	$(CARGO) clean
