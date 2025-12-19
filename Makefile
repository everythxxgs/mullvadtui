PREFIX ?= /usr/local
BINDIR = $(PREFIX)/bin
SUDOERS_FILE = /etc/sudoers.d/mvtui

.PHONY: build install uninstall clean

build:
	cargo build --release

install: build
	install -Dm755 target/release/mvtui $(DESTDIR)$(BINDIR)/mvtui
	@echo "$(SUDO_USER) ALL=(root) NOPASSWD: $(BINDIR)/mvtui" > $(SUDOERS_FILE)
	@chmod 440 $(SUDOERS_FILE)
	@echo "Creating wrapper script..."
	@mv $(DESTDIR)$(BINDIR)/mvtui $(DESTDIR)$(BINDIR)/mvtui-bin
	@echo '#!/bin/sh' > $(DESTDIR)$(BINDIR)/mvtui
	@echo 'exec sudo $(BINDIR)/mvtui-bin "$$@"' >> $(DESTDIR)$(BINDIR)/mvtui
	@chmod 755 $(DESTDIR)$(BINDIR)/mvtui
	@echo "Installed! Just run: mvtui"

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/mvtui
	rm -f $(DESTDIR)$(BINDIR)/mvtui-bin
	rm -f $(SUDOERS_FILE)

clean:
	cargo clean
