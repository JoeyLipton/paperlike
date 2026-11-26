# make                  build
# make install          install to ~/.local (no root needed)
# sudo make install PREFIX=/usr/local
#
# DESTDIR is supported for packaging.

PREFIX ?= $(HOME)/.local
BIN := target/release/paperlike

.PHONY: build install uninstall test

build:
	cargo build --release --locked

test:
	cargo test --locked

install:
	@test -x $(BIN) || $(MAKE) build
	install -Dm755 $(BIN) $(DESTDIR)$(PREFIX)/bin/paperlike
	install -Dm644 assets/paperlike.desktop $(DESTDIR)$(PREFIX)/share/applications/paperlike.desktop
	install -Dm644 doc/paperlike.1 $(DESTDIR)$(PREFIX)/share/man/man1/paperlike.1

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/paperlike
	rm -f $(DESTDIR)$(PREFIX)/share/applications/paperlike.desktop
	rm -f $(DESTDIR)$(PREFIX)/share/man/man1/paperlike.1
