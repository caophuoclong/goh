# Changelog

## [2.1.1](https://github.com/caophuoclong/goh/compare/v2.1.0...v2.1.1) (2026-10-31)


### Bug fixes

* **daemon:** list slow Docker containers and read the shell's own HISTFILE ([fe665f4](https://github.com/caophuoclong/goh/commit/fe665f464988315c914af207e771fc387014a6bd))
* **daemon:** list slow Docker containers and read the shell's own HISTFILE ([173725d](https://github.com/caophuoclong/goh/commit/173725dbc60ea1e9a6916b562dbcc295d5322cfe))

## [2.1.0](https://github.com/caophuoclong/goh/compare/v2.0.0...v2.1.0) (2026-10-31)


### Features

* **history:** offer zsh history lines with their secrets masked ([bef8b64](https://github.com/caophuoclong/goh/commit/bef8b64faf4a1343b3f48aae6aea1474fab43e2e))
* **zsh:** close the menu on Esc at once and offer history secrets masked ([a8fab80](https://github.com/caophuoclong/goh/commit/a8fab8049bcd6ddc0be8a1b5e302e60403d36407))


### Bug fixes

* **zsh:** close the menu on Esc at once ([b1b63d6](https://github.com/caophuoclong/goh/commit/b1b63d65063436ffdbba84506a8903aeee59dfb7))

## [2.0.0](https://github.com/caophuoclong/goh/compare/v1.11.0...v2.0.0) (2026-10-31)


### ⚠ BREAKING CHANGES

* **zsh:** Enter on a suggestion you moved to, or on a lone one, now runs the line instead of only inserting the suggestion. Set display.enterRuns to false in `goh config` to keep the old behaviour.

### Features

* **popup:** give Goh.app its icon ([432c533](https://github.com/caophuoclong/goh/commit/432c533b07aeb9f3c578b453b990a7a4fc548cce))
* **popup:** name the macOS app Goh, with a slot for its icon ([7241a6a](https://github.com/caophuoclong/goh/commit/7241a6a4b0e524504f8644bd794a5d17d372fad3))
* **zsh:** Enter takes the picked suggestion and runs the line ([49c6194](https://github.com/caophuoclong/goh/commit/49c6194c9170869bd9ca477702aad162dd3c3837))


### Bug fixes

* **popup:** check the permission again before asking for it ([190e208](https://github.com/caophuoclong/goh/commit/190e208cbb7baa5a7067c1638e0670e588a42a8a))

## [1.11.0](https://github.com/caophuoclong/goh/compare/v1.10.2...v1.11.0) (2026-10-31)


### Features

* draw the popup over ssh with `goh ssh` ([6bd2c10](https://github.com/caophuoclong/goh/commit/6bd2c107cea0d6cb0c731819c9b48cb5ceb3429c))
* **risk:** warn about database, data-deleting and publishing commands, in red ([e443c94](https://github.com/caophuoclong/goh/commit/e443c94d7659514c0fe94ebda1bea754c58eabfc))
* **ssh:** offer a session-only copy, ask in a highlighted question ([0dfeac0](https://github.com/caophuoclong/goh/commit/0dfeac04750d646872ea7590c3eb5899cd9eee44))
* **ssh:** wrap ssh and offer goh to hosts that have none ([c6d0ea4](https://github.com/caophuoclong/goh/commit/c6d0ea47341b14bed66be99f15d2a8b2f3431ab5))

## [1.10.2](https://github.com/caophuoclong/goh/compare/v1.10.1...v1.10.2) (2026-10-30)


### Bug fixes

* **popup:** keep goh-popup's macOS permission across updates ([c1e8504](https://github.com/caophuoclong/goh/commit/c1e85045fe4c4972a62e02ba670aa5eeee2f644a))

## [1.10.1](https://github.com/caophuoclong/goh/compare/v1.10.0...v1.10.1) (2026-10-30)


### Bug fixes

* **core:** rank directories by their last component ([0081951](https://github.com/caophuoclong/goh/commit/008195155686a19fd78f47f3190c2a5b9f2fd097))

## [1.10.0](https://github.com/caophuoclong/goh/compare/v1.9.0...v1.10.0) (2026-10-30)


### Features

* **zsh:** Enter accepts a lone suggestion; document Linux install ([7bb7422](https://github.com/caophuoclong/goh/commit/7bb7422c20d602d24344b5e4ce33a08fc216e52c))

## [1.9.0](https://github.com/caophuoclong/goh/compare/v1.8.0...v1.9.0) (2026-10-30)


### Features

* enhance command prediction and history suggestions in zsh adapt… ([4e02064](https://github.com/caophuoclong/goh/commit/4e0206422db50f64e7ad36d4c81384cd6c72703d))
* enhance command prediction and history suggestions in zsh adapter and config ([c74a052](https://github.com/caophuoclong/goh/commit/c74a05218579b9ae32778b85b19b0eeabc04ac92))

## [1.8.0](https://github.com/caophuoclong/goh/compare/v1.7.1...v1.8.0) (2026-10-30)


### Features

* **anchor:** add capability-driven Linux X11 and AT-SPI caret provider ([fe86e04](https://github.com/caophuoclong/goh/commit/fe86e04f5ff00b242772867eb7ae8f3b271a628a))
* **popup:** add focus-preserving X11 surfaces and Linux font lookup ([b8422fa](https://github.com/caophuoclong/goh/commit/b8422faeff6211eaa3548e8482b0e12a35cb3f0c))


### Bug fixes

* **anchor:** reject ambiguous Linux terminal panes and cover fallback ([430ea94](https://github.com/caophuoclong/goh/commit/430ea944e40eb439ccccfcb9b2137e89971ff116))
* **install:** include popup in Linux source installations ([6e035a0](https://github.com/caophuoclong/goh/commit/6e035a00bcf64a4163557ad8cd174926c3d1aec7))


### Performance

* **anchor:** reuse Linux X11 and AT-SPI connections across caret checks ([f021ad8](https://github.com/caophuoclong/goh/commit/f021ad8b5766774e1b1dc40e3bcbcdd5fa6bc50f))

## [1.7.1](https://github.com/caophuoclong/goh/compare/v1.7.0...v1.7.1) (2026-10-29)


### Bug fixes

* **anchor:** switch on VS Code's accessibility tree ([6b345ae](https://github.com/caophuoclong/goh/commit/6b345aef15b90e68a47158a05b056cfc012c25cd))
* **anchor:** switch on VS Code's accessibility tree ([4df972d](https://github.com/caophuoclong/goh/commit/4df972d76f198e4b35aacfb6219617baaf2f9945))

## [1.7.0](https://github.com/caophuoclong/goh/compare/v1.6.0...v1.7.0) (2026-10-29)


### Features

* completion features users asked Fig for (wrappers, git aliases, kubectl, project scripts, 18 specs) ([1f97ace](https://github.com/caophuoclong/goh/commit/1f97ace8b25b754287ff93d2fc499424fed9b933))

## [1.6.0](https://github.com/caophuoclong/goh/compare/v1.5.0...v1.6.0) (2026-10-29)


### Features

* **config:** popup.ownPermission turns on goh-popup's own permission ([3ea080e](https://github.com/caophuoclong/goh/commit/3ea080e68971e93ac6aaaa34d10252905bb4bbeb))
* history ghost text from the zsh history, smarter history lines, and goh-popup's own permission ([a378b9a](https://github.com/caophuoclong/goh/commit/a378b9adab113bd7aa602036edd3bd879de02732))
* **history:** do not offer again a line that did its job once ([e4b7399](https://github.com/caophuoclong/goh/commit/e4b73991b4f717293029cfeaef2ac5f27279943a))
* **history:** read the zsh history and show its best line as ghost text ([df56c3c](https://github.com/caophuoclong/goh/commit/df56c3c56314fdc5cd540b0adaf82a51171cd988))
* **popup:** opt-in Accessibility permission for goh-popup itself ([32db3f6](https://github.com/caophuoclong/goh/commit/32db3f6e327c75feb79e23d4fe402c7fc929279f))


### Bug fixes

* **zsh:** run other plugins' widgets without goh's no_unset ([d719461](https://github.com/caophuoclong/goh/commit/d71946138b89c6e4def775495ed21930f6ab14f0))

## [1.5.0](https://github.com/caophuoclong/goh/compare/v1.4.1...v1.5.0) (2026-10-29)


### Features

* opt-in notice when a newer goh release exists ([bf87f90](https://github.com/caophuoclong/goh/commit/bf87f90e18faba894c90dd22d9dec5207e8a06c9))


### Bug fixes

* **daemon:** wait for a just-started daemon to answer before the first request ([1025b9f](https://github.com/caophuoclong/goh/commit/1025b9f44502468d3231fe79c6cd1026ffbef354))
* wait for a cold-started daemon to answer, and harden the workflows for zizmor ([fa91e10](https://github.com/caophuoclong/goh/commit/fa91e10fcfea0fb4ddbb7e8298ebec0e1d1649b7))
* **zsh:** hide the popup on Ctrl+C even when a plugin replaces zle-line-init ([033875f](https://github.com/caophuoclong/goh/commit/033875f542001abbd2e64e4bfc4f0c6b17227fd8))
* **zsh:** hide the popup on Ctrl+C even when a plugin replaces zle-line-init ([710f7f4](https://github.com/caophuoclong/goh/commit/710f7f4ea16e074935a04038f0c7ffd2604fb57a))

## [1.4.1](https://github.com/caophuoclong/goh/compare/v1.4.0...v1.4.1) (2026-10-28)


### Bug fixes

* **zsh:** keep the inline menu visible with zsh-autosuggestions, and reload the adapter on goh reload ([3e85a3a](https://github.com/caophuoclong/goh/commit/3e85a3af7a2e52f715e7daa4f2410856ee658719))
* **zsh:** keep the inline menu visible with zsh-autosuggestions, and reload the adapter on goh reload ([bf54234](https://github.com/caophuoclong/goh/commit/bf542342ed62abcbe7ff0d8f96b2c7e4959043a5))

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
