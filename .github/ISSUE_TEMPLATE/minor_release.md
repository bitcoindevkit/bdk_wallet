---
name: Minor Release
about: Create a new minor release [for release managers only]
title: 'Release v{MAJOR}.{MINOR+1}.0'
labels: 'release'
assignees: ''

---

## Create a new minor release

### Summary

<--release summary to be used in announcements-->

### Commit

<--latest commit ID to include in this release-->

### Changelog

<--add notices from PRs merged since the prior release, see ["keep a changelog"]-->

### Checklist

Release numbering must follow [Semantic Versioning].

These steps assume that the remote pointing to `bitcoindevkit/bdk_wallet` is named `origin`.

<!-- These steps assume the current `master` branch **development** version is *MAJOR.MINOR.0*. -->

#### On the day of the feature freeze

Change the `v{MAJOR}` branch to the next MAJOR.MINOR+1 version:

- [ ] Make sure the remote `v{MAJOR}` branch is up to date: `git fetch origin v{MAJOR}`.
- [ ] Create and switch to a new local branch based on `origin/v{MAJOR}`: `git checkout -b bump/v{MAJOR}.{MINOR+1}.0 origin/v{MAJOR}`.
- [ ] Bump version in files to the next development MINOR+1 version.
  - Change `Cargo.toml` version value to `{MAJOR}.{MINOR+1}.0`.
  - Update the `CHANGELOG.md` file.
  - Commit with message: "Bump version to {MAJOR}.{MINOR+1}.0".
- [ ] Create PR and merge the `bump/v{MAJOR}.{MINOR+1}.0` branch to `v{MAJOR}`.
  - Title PR "Bump version to {MAJOR}.{MINOR+1}.0".

Create a release candidate `rc.{x}`:

- [ ] Make sure the remote `v{MAJOR}` branch is up to date: `git fetch origin v{MAJOR}`.
- [ ] Create and switch to a new local branch based on `origin/v{MAJOR}`: `git checkout -b release/v{MAJOR}.{MINOR+1}.0-rc.{x} origin/v{MAJOR}`.
- [ ] Bump version in files:
  - Change `Cargo.toml` version value to `{MAJOR}.{MINOR+1}.0-rc.{x}`.
  - Commit with message: "Bump version to {MAJOR}.{MINOR+1}.0-rc.{x}".
- [ ] Create PR and merge the `bump/v{MAJOR}.{MINOR+1}.0-rc.{x}` branch to `v{MAJOR}`.
  - Title PR "Release {MAJOR}.{MINOR+1}.0-rc.{x}".
  - Ensure `cargo publish --dry-run` succeeds.
- [ ] Tag the release after the PR is merged.
  - Make sure remote is up to date: `git fetch origin v{MAJOR}`.
  - `git tag -a v{MAJOR}.{MINOR+1}.0-rc.{x} origin/v{MAJOR} -m "Tag release {MAJOR}.{MINOR+1}.0-rc.{x}"`.
  - Make sure the tag is signed (can use `--sign` flag).
  - Push tag with `git push origin v{MAJOR}.{MINOR}.0-rc.{x}`.
- [ ] Cargo publish.

If any issues need to be fixed before the *`{MAJOR}.{MINOR+1}.0`* version is released:

- [ ] Merge fix PRs to the `master` branch.
- [ ] Create PR that cherry-picks fixes into the `v{MAJOR}` branch.
  - PR description: "Backport of #{TICKET_NUMBER} to `v{MAJOR}`".
- [ ] Release candidate version `rc.{x+1}`.

#### On the day of the release

Tag and publish new release:

- [ ] Create & merge PR into `v{MAJOR}` that bumps version to `{MAJOR}.{MINOR+1}.0`.
  - Change `Cargo.toml` version value to `{MAJOR}.{MINOR}.0`.
  - Commit message: "Bump version to {MAJOR}.{MINOR}.0".
  - Ensure `cargo publish --dry-run` succeeds.
  - Remember to merge PR before continuing.
- [ ] Tag the `HEAD` of the `origin/v{MAJOR}` branch.
  - Tag name: `v{MAJOR}.{MINOR+1}.0`.
  - Tag message:
    - Title: `Release {MAJOR}.{MINOR+1}.0`.
    - Body: Copy of the **Summary** and **Changelong**.
      - Make sure the tag is signed (can use `--sign`).
  - Push the tag to `origin`.
- [ ] Create the release on GitHub.
  - Go to "tags", click on the dots on the right and select "Create Release".
  - Set the title to `Release {MAJOR}.{MINOR+1}.0`.
  - In the release notes body put the **Summary** and **Changelog**.
  - Use the "+ Auto-generate release notes" button to add details from included PRs.
- [ ] Cargo publish.
  - Make sure the new release shows up on [crates.io] and that the docs are built correctly on [docs.rs].
- [ ] Announce the release, using the **Summary**, on Discord, Twitter and Mastodon.
- [ ] Celebrate 🎉

[Semantic Versioning]: https://semver.org/
[crates.io]: https://crates.io/crates/bdk
[docs.rs]: https://docs.rs/bdk/latest/bdk
["keep a changelog"]: https://keepachangelog.com/en/1.0.0/
