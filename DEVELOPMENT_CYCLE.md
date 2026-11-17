# Development Cycle

This project follows a regular releasing schedule similar to the one [used by the Rust language]. In short, this means that a new release is made at a regular cadence, with all the feature/bugfixes that made it to `master` in time. This ensures that we don't keep delaying releases waiting for "just one more little thing".

## Releasing

Releases are prepared and published with [release-plz] from the **Release-plz** GitHub Actions workflow. The workflow is triggered manually from the branch to release: `master` for regular releases or a `release/*` branch for patch and pre-releases.

1. Trigger the **Release-plz** workflow on the branch. The `release-pr` job opens or updates a release PR containing the version bump and `CHANGELOG.md` changes.
2. Review the release PR. Changelogs are generated from [Conventional Commits], so do not edit them by hand. Merge the PR once CI passes.
3. Trigger the **Release-plz** workflow again on the same branch. The `release` job publishes the crate to crates.io if the version is unpublished, pushes a `wallet-<version>` tag, and creates the matching GitHub release. A reviewer must approve the `release` environment before publishing starts.
4. Verify the new version on crates.io and docs.rs, then announce the release.

release-plz supports only one open release PR per repository, so do not prepare releases on two branches at the same time.

For a patch release, cherry-pick the fixes onto the appropriate `release/*` branch before triggering the workflow. Make sure the crate version on the selected branch is not behind the version already published on crates.io; release-plz uses crates.io as its baseline.

Before the `release` job can publish, [crates.io trusted publishing] must be configured for the crate with workflow name `release-plz.yml` and environment `release`.

[release-plz]: https://release-plz.dev
[Conventional Commits]: https://www.conventionalcommits.org
[crates.io trusted publishing]: https://crates.io/docs/trusted-publishing

[used by the Rust language]: https://doc.rust-lang.org/book/appendix-07-nightly-rust.html
[Semantic Versioning]: https://semver.org/
