# Forest Test Plan

    Version: 1.0
    Author: David Himmelstrup
    Date updated: 2022-11-14

## Test objective:

The Filecoin specification is complex and changes rapidly over time. To manage this complexity, Forest uses a rigorous testing framework, starting with individual functions and ending with complete end-to-end validation. The goals, in descending order of priority, are:

- **Regression detection.** If Forest can no longer connect to mainnet or if any of its features break, the development team should be automatically notified.
- **No institutional/expert knowledge required.** Developers can work on a Forest subsystem without worrying about accidentally breaking a different subsystem.
- **Bug identification.** If something break, the test data should narrow down the location of the issue.


## Scope of testing definition:

Forest testing is multifaceted and layered. The testing pipeline looks like this:

- **Unit tests for library functions.** Example: parsing a network version fails for garbled input.
- **Unit tests for CLI programs.** Example: `forest-cli dump` produces a valid configuration.
- **Property tests.** Example: `deserialize ∘ serialize = id` for all custom formats.
- **Network synchronization.** PRs are checked against the calibration network, the main branch is checked against the main network.
- **End-to-end feature tests.** Example: Network snapshots are generated daily and hosted publicly.
- **Link checking.** API documentation and markdown files are checked for dead links.
- **Spell checking.** API documentation is checked for spelling errors and typos.

All testing is automated and there are no additional manual checks required for releases.

## Resources / Roles & Responsibilities:

Testing is a team effort and everyone is expected to add unit tests, property tests, or integration tests as part of their PR contributions.

## Tools description:

- Bug tracker: https://github.com/ChainSafe/forest/issues
- Test Automation tools: [nextest](https://nexte.st/), [quickcheck](https://docs.rs/quickcheck/latest/quickcheck/)
- Languages: [Rust](https://www.rust-lang.org/)
- CI/CD: [GitHub Actions](https://github.com/ChainSafe/forest/actions)
- Version control: [Git](https://git-scm.com/)

## Deliverables:

The only deliverable is a green checkmark. Either all tests pass and a PR may be merged into the main branch or something is not up to spec and the PR is blocked.

## Test Environment & CI

Short-running tests are executed via GitHub Actions on Linux and MacOS. Long-running tests are run on dedicated testing servers.

The services on the dedicated servers are described here: https://github.com/ChainSafe/forest-iac

## Test Data:

No private or confidential data is involved in testing. Everything is public.

## Bug template:

Bug report template is available on GitHub: https://github.com/ChainSafe/forest/blob/main/.github/ISSUE_TEMPLATE/bug_report.md

The template is applied automatically when bugs are reported through GitHub.

## Risk & Issues:

- We depend on the calibration network for testing. If this network is down, our testing capabilities are degraded.
- We depend on GitHub Actions for testing. If GitHub Action is unavailable, testing will be degraded.
- Testing against mainnet is effective for discovering issues, but not great for identifying root causes. Finding bugs *before* syncing to mainnet is always to be preferred.
