# Binary / SwiftPM target (no spaces) vs. user-facing app bundle name.
APP = VillainCaster
APP_NAME = Caster
BUNDLE = build/$(APP_NAME).app
# Stable identity keeps TCC grants (Screen Recording, Accessibility) valid
# across rebuilds; ad-hoc ("-") re-prompts after every install.
SIGN_IDENTITY = $(shell security find-identity -v -p codesigning 2>/dev/null | grep -q "VillainCaster Dev" && echo "VillainCaster Dev" || echo -)

.PHONY: build run app icon install release clean design design-check

build:
	swift build -c release

run: build
	rm -rf .build/release/Fonts
	cp -R Resources/Fonts .build/release/Fonts
	./.build/release/$(APP)

app: build
	rm -rf "$(BUNDLE)"
	mkdir -p "$(BUNDLE)/Contents/MacOS"
	cp .build/release/$(APP) "$(BUNDLE)/Contents/MacOS/"
	mkdir -p "$(BUNDLE)/Contents/Resources"
	cp Resources/Info.plist "$(BUNDLE)/Contents/"
	cp Resources/AppIcon.icns "$(BUNDLE)/Contents/Resources/"
	cp -R Resources/Fonts "$(BUNDLE)/Contents/Resources/Fonts"
	codesign --force --sign "$(SIGN_IDENTITY)" "$(BUNDLE)"
	@echo "Built $(BUNDLE)"

# Re-render Resources/AppIcon.icns from Resources/AppIcon.svg (committed, so
# only needed after editing the SVG). Each size is rendered, not downscaled.
ICONSET = build/AppIcon.iconset
RENDER = build/render-svg
icon:
	rm -rf $(ICONSET)
	mkdir -p $(ICONSET)
	swiftc -O -o $(RENDER) scripts/render-svg.swift
	for s in 16 32 128 256 512; do \
		$(RENDER) Resources/AppIcon.svg $(ICONSET)/icon_$${s}x$${s}.png $$s && \
		$(RENDER) Resources/AppIcon.svg $(ICONSET)/icon_$${s}x$${s}@2x.png $$((s * 2)) || exit 1; \
	done
	iconutil -c icns $(ICONSET) -o Resources/AppIcon.icns
	@echo "Wrote Resources/AppIcon.icns"

# Remove legacy bundle names when installing the renamed Caster app.
install: app
	-pkill -x $(APP)
	rm -rf "/Applications/$(APP).app"
	rm -rf "/Applications/Villain Caster.app"
	rm -rf "/Applications/$(APP_NAME).app"
	cp -R "$(BUNDLE)" /Applications/
	open "/Applications/$(APP_NAME).app"
	@echo "Installed and started /Applications/$(APP_NAME).app"

# Zip for a Forgejo release. Not notarized (no paid Apple Developer ID), so
# first launch needs Privacy & Security → Open Anyway. Signed with the stable
# "VillainCaster Dev" identity when present so users' permission grants
# survive updates.
VERSION = $(shell /usr/libexec/PlistBuddy -c "Print CFBundleShortVersionString" Resources/Info.plist)
# <name>-<version>-<os>-<arch>, e.g. Villain-Caster-0.2.0-macos-arm64.zip.
ARCH = $(shell uname -m)
ZIP = build/Villain-Caster-$(VERSION)-macos-$(ARCH).zip

release: app
	codesign --verify --strict --verbose=2 "$(BUNDLE)"
	rm -f "$(ZIP)"
	ditto -c -k --keepParent "$(BUNDLE)" "$(ZIP)"
	@echo "Wrote $(ZIP)"

# Caster keeps its own copy of the vault's design sources, so it builds on
# its own: the generated Swift tokens, Atkinson Hyperlegible Next with its
# license, and the C mark. `make design` refreshes the copy; `make
# design-check` writes nothing and fails if it has drifted.
VAULT = ../../..
DESIGN = Sources/VillainCaster/Design
FONTS = Resources/Fonts
DESIGN_COPIES = \
	$(VAULT)/studio/design/tokens/color.swift:$(DESIGN)/color.swift \
	$(VAULT)/studio/design/tokens/type.swift:$(DESIGN)/type.swift \
	$(VAULT)/studio/design/tokens/shape.swift:$(DESIGN)/shape.swift \
	$(VAULT)/code/apps/site/assets/fonts/atkinson-hyperlegible-next.ttf:$(FONTS)/atkinson-hyperlegible-next.ttf \
	$(VAULT)/code/apps/site/assets/fonts/atkinson-hyperlegible-next-OFL.txt:$(FONTS)/atkinson-hyperlegible-next-OFL.txt \
	$(VAULT)/studio/brand/mark/caster.svg:Resources/AppIcon.svg
# The menu bar draws the C from the mark's own outline.
MARK_PATH = $(shell sed -n 's/.*id="c" d="\([^"]*\)".*/\1/p' $(VAULT)/studio/brand/mark/caster.svg)

design:
	mkdir -p $(DESIGN) $(FONTS)
	@for pair in $(DESIGN_COPIES); do cp "$${pair%%:*}" "$${pair#*:}"; done
	@echo "Synced tokens, fonts and the mark. If Resources/AppIcon.svg changed, run make icon."

design-check:
	@stale=""; for pair in $(DESIGN_COPIES); do \
		cmp -s "$${pair%%:*}" "$${pair#*:}" || stale="$$stale $${pair#*:}"; \
	done; \
	grep -qF "$(MARK_PATH)" Sources/VillainCaster/MenuBarIcon.swift || stale="$$stale Sources/VillainCaster/MenuBarIcon.swift"; \
	if [ -n "$$stale" ]; then echo "Out of date with the vault:$$stale. Run make design (and copy the mark's C path into MenuBarIcon.swift)."; exit 1; fi

clean:
	rm -rf .build build
