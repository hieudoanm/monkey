.PHONY: test format-test grammar-test install package clean

test: format-test grammar-test

format-test:
	node --test formatter/

grammar-test:
	cd grammars/tree-sitter-jungle && npx --yes tree-sitter-cli@0.27.1 test

install:
	./scripts/install.sh

# Declarative extension: ship the source tree as a tarball — Zed installs
# extensions from .tar.gz archives or git repos, not .zip.
package:
	rm -rf download
	mkdir -p download/pkg
	cp -R extension.toml LICENSE README.md formatter grammars icon_themes \
		icons languages samples scripts themes download/pkg
	tar -czf download/extension-zed-monkey-c.tar.gz -C download/pkg \
		--exclude='*/node_modules' --exclude='*/target' .
	rm -rf download/pkg

clean:
	rm -rf download
