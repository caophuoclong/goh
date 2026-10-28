# Changelog

## [1.4.0](https://github.com/caophuoclong/goh/compare/v1.3.0...v1.4.0) (2026-10-28)


### Features

* **zsh:** rework the inline menu with kind glyphs, warnings on every row and a detail line ([a701e4a](https://github.com/caophuoclong/goh/commit/a701e4a35a9f852a5b5f24d775cbebf25bdb1d68))

## [1.3.0](https://github.com/caophuoclong/goh/compare/v1.2.0...v1.3.0) (2026-10-28)


### Features

* **doctor:** check the adapter and daemon version, add --test, --report, logs and specs --missing ([596693d](https://github.com/caophuoclong/goh/commit/596693d0d7aec4ff8555912d5a8271b1c2f5e0f8))
* richer goh doctor, daemon log, and 39 more bundled specs ([24ed044](https://github.com/caophuoclong/goh/commit/24ed044f08bcc5e38bd5df22ce659c8b77058a71))
* **specs:** bundle 39 more common commands ([3731440](https://github.com/caophuoclong/goh/commit/3731440ae26b5f95a1b47b3898fd7fbce4d6e46c))

## [1.2.0](https://github.com/caophuoclong/goh/compare/v1.1.3...v1.2.0) (2026-10-28)


### Features

* **spec:** suggest package names for brew, npm, pnpm, yarn, bun, cargo, pip and uv ([8246a74](https://github.com/caophuoclong/goh/commit/8246a7427c4efaee3757482bb30fc09199307fc2))
* **spec:** suggest package names for brew, npm, pnpm, yarn, bun, cargo, pip and uv ([c960b86](https://github.com/caophuoclong/goh/commit/c960b8686dc5999cb617abb10d14876ecc04e2ec))

## [1.1.3](https://github.com/caophuoclong/goh/compare/v1.1.2...v1.1.3) (2026-10-28)


### Bug fixes

* **anchor:** fall back to the terminal's host app when Claude hides its accessibility tree ([f022a4b](https://github.com/caophuoclong/goh/commit/f022a4bf2f269b7e5120cb33c053ceecf757e416))
* zsh cd alias and redraw fixes, daemon upgrade detection, Claude popup anchor ([3d80c4b](https://github.com/caophuoclong/goh/commit/3d80c4bb4055bac5089569182cb893708b671768))

## [1.1.2](https://github.com/caophuoclong/goh/compare/v1.1.1...v1.1.2) (2026-10-28)


### Bug fixes

* cd with a zoxide alias, Tab redraw and dimmed text in zsh ([2e818ab](https://github.com/caophuoclong/goh/commit/2e818ab2712d2113118430ca55ea2c139c6d5490))
* **core:** keep an alias's own spec when its expansion has none ([bacc726](https://github.com/caophuoclong/goh/commit/bacc726c6a9a03c3c9746717d36fdc28ef42ef06))
* **daemon:** recognise a daemon whose binary an upgrade replaced ([2193727](https://github.com/caophuoclong/goh/commit/2193727278e9e2124812a2fc60c8d5497e6870b3))
* **zsh:** do not dim accepted text with a suggestion's highlight ([dfb75a1](https://github.com/caophuoclong/goh/commit/dfb75a1c8ac480d4c8554156e11dca92404a0fa0))
* **zsh:** keep the line in place when a directory reopens the menu ([efe88c2](https://github.com/caophuoclong/goh/commit/efe88c2c435dcace4d507aad637121f7005b2d63))

## [1.1.1](https://github.com/caophuoclong/goh/compare/v1.1.0...v1.1.1) (2026-10-28)


### Bug fixes

* **zsh:** reopen the menu after accepting a directory ([e72d1c4](https://github.com/caophuoclong/goh/commit/e72d1c4764a7ac40e596a1d3c1568d0a03b519f6))

## [1.1.0](https://github.com/caophuoclong/goh/compare/v1.0.1...v1.1.0) (2026-10-27)


### Features

* goh setup adds the adapter to your shell file ([13d357a](https://github.com/caophuoclong/goh/commit/13d357a0af43f6dcb036e8c2cae42197f77d04ac))

## [1.0.1](https://github.com/caophuoclong/goh/compare/v1.0.0...v1.0.1) (2026-10-27)


### Bug fixes

* install.sh verifies the download against SHA256SUMS ([d360fc8](https://github.com/caophuoclong/goh/commit/d360fc893d9439ec2e277b942ab280dfd83cf5b4))
