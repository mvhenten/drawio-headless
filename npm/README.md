# drawio-headless

CLI wrapper that installs the pre-built [`drawio-headless`][repo] Rust
binary for your platform.

```sh
npm install -g drawio-headless
drawio-headless --version
```

The `postinstall` step downloads the matching binary from the
[GitHub Releases page][releases] for this package version into the
package's `vendor/` directory; the `drawio-headless` `bin` entry is a
small Node shim that `spawn`s it with the caller's argv.

## Supported platforms

| OS      | Architecture     |
| ------- | ---------------- |
| Linux   | x86\_64, aarch64 |
| macOS   | x86\_64, arm64   |
| Windows | x86\_64          |

If your platform isn't in this list, `npm install` will fail clearly and
point you at `cargo install` as the fallback.

## Usage

See the [main project README][repo] for `render`, `author`, `compose`,
and `list-shapes` examples.

## License

Apache-2.0. See [LICENSE][license].

[repo]: https://github.com/mvhenten/drawio-headless
[releases]: https://github.com/mvhenten/drawio-headless/releases
[license]: https://github.com/mvhenten/drawio-headless/blob/main/LICENSE
