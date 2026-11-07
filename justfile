[doc("list recipes")]
default:
    @just --list --unsorted

[doc("build production files into dist/")]
build:
    cargo build --release
    mkdir -p dist
    install -m755 "${CARGO_TARGET_DIR:-target}/release/glimpse-monitor" dist/
    cp lt.nyv.glimpse-monitor.desktop dist/

[doc("run the applet in the panel, restarting on source edits")]
dev zone="left":
    glimpsectl applet run --dev --zone {{ zone }} .

[doc("build the package and install it")]
install:
    glimpsectl applet package --install .

[doc("build the applet package")]
package:
    glimpsectl applet package .

[doc("run the tests")]
test:
    cargo test

[doc("lint")]
lint:
    cargo clippy --all-targets -- -D warnings
