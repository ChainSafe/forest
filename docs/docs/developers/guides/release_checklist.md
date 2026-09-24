# Release checklist

Forest doesn't follow a fixed schedule but releases should be expected at least
quarterly. A _release officer_ is volunteered for each release, and they are
responsible for either following the checklist or, in case of absence, passing
the task to a different team member.

## Prepare the release

Make a pull request with the following changes:

- Update the CHANGELOG.md file to reflect all changes and preferably write a
  small summary about the most notable updates. The changelog should follow the
  design philosophy outlined [here][1]. Go through the output of
  `git log <last-tag>..HEAD` and remember that the audience of the CHANGELOG
  does not have intimate knowledge of the Forest code-base. All the
  changed/updated/removed features should be reasonably understandable to an
  end-user.
- Update the version of the [forest crate][2] (and any others, if applicable) to
  be released. Bump the **minor** version (`0.Y.0`) for incompatible/breaking
  changes and/or network upgrades, otherwise (non-breaking features, patches,
  etc.) bump the **patch** version (`0.Y.Z`). Make sure
  that the updated files do **not** contain a `[patch.crates-io]` section,
  otherwise you won't be able to make a release on
  [crates.io](https://crates.io/).
- Make sure to run `cargo publish --dry-run` and include the `Cargo.lock` crate
  version change in the release.
- Make sure to update RPC specs by running `mise insta`
- The Pull Request must have the `Release` label.

## Release on GitHub

> [!IMPORTANT]
> The GitHub release must be created only after the release pull request is merged.

- Create a [new release][4]. Click on `Choose a tag` button and create a new
  one. The tag must start with a lowercase `v`, e.g., `v0.11.0`. Follow the
  title convention of the previous releases, and write a small summary of the
  release (similar or identical to the summary in the [CHANGELOG.md][5] file).
  Add additional, detailed notes with `Generate release notes` button.
- Verify that the new release contains assets for both Linux and macOS (the
  assets are automatically generated and should show up after 30 minutes to an
  hour).
- 🔁 If it's a new stable release (and not a backport), tag the version as
  `latest` with the [retag action][6].
- Verify that the new release is available in the GitHub Container Registry. Use
  `docker pull ghcr.io/chainsafe/forest:<version>`. Verify the tags in the
  [packages][7] list.
- Verify that the new release is published to [crates.io](https://crates.io/crates/forest-filecoin).

## Network upgrade releases

> [!IMPORTANT]
> This section applies only to a release that adds support for a network upgrade
> (NVXX). Skip it for regular releases.

A network upgrade needs two releases: one ahead of the calibnet upgrade and one
ahead of the mainnet upgrade. Both are announced in a single GitHub discussion. The latest
posts are in the [Announcements category][8]; the [NV29 post][9] is a complete
example.

### Open the announcement discussion (calibnet release)

On the day of the calibnet release, create a discussion in the
`Announcements 📢` category titled `Forest NVXX support`. Copy the
previous post and replace every detail:

- The link to the upstream post with the upgrade scope and dates (the Core Devs
  planning discussion, or the final Filecoin community post it points to).
- The `mermaid` timeline: Forest calibnet release, calibnet upgrade with its
  epoch and UTC time, Forest mainnet release, mainnet upgrade. Prefix dates that
  are not final with `~`.
- Hardware requirements: state migration duration and peak `RSS` for calibnet and
  mainnet. Measure them with `forest-tool shed migrate-state` as described in
  the [state migration guide][10]; do not reuse the numbers from the previous
  upgrade.
- The link to the [network upgrades knowledge base page][11].

### Comment on the discussion after each release

Once the release is published, reply in the discussion (see the
[NV29 calibnet comment][12]) with:

- the release link, and the upgrade epoch and UTC time before which operators
  must upgrade; write **mainnet** in bold in the mainnet comment,
- links to the release assets, the `<version>-fat` image in the [packages][7]
  list, and crates.io,
- a picture that plays on the release name, with a link to its source.

After the post and after each comment, share the discussion link in the
`#fil-forest-announcements` channel of the Filecoin Slack, re-share it with the
ChainSafe Infra team, and tick the matching box in the tracking issue. Edit the
timeline in the post once the mainnet dates are final.

[1]: https://keepachangelog.com/en/1.0.0/
[2]: https://github.com/ChainSafe/forest/blob/main/Cargo.toml
[3]: https://doc.rust-lang.org/cargo/reference/publishing.html
[4]: https://github.com/ChainSafe/forest/releases/new
[5]: https://github.com/ChainSafe/forest/blob/main/CHANGELOG.md
[6]: https://github.com/ChainSafe/forest/actions/workflows/docker-latest-tag.yml
[7]: https://github.com/ChainSafe/forest/pkgs/container/forest
[8]: https://github.com/ChainSafe/forest/discussions/categories/announcements
[9]: https://github.com/ChainSafe/forest/discussions/7670
[10]: ./state_migration_guide.md
[11]: https://docs.forest.chainsafe.io/knowledge_base/network_upgrades_state_migrations/
[12]: https://github.com/ChainSafe/forest/discussions/7670#discussioncomment-18579988
