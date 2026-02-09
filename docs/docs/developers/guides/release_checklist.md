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
  be released. Make sure that the updated files do **not** contain a
  `[patch.crates-io]` section, otherwise you won't be able to make a release on
  [crates.io](https://crates.io/).
- Make sure to run `cargo publish --dry-run` and include the `Cargo.lock` crate
  version change in the release.
- Make sure to update RPC specs by running
  ```
  cargo test --lib -- rpc::tests::openrpc
  cargo insta review
  ```
- The Pull Request must have the `Release` label.

## Release on GitHub

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

[1]: https://keepachangelog.com/en/1.0.0/
[2]: https://github.com/ChainSafe/forest/blob/main/Cargo.toml
[3]: https://doc.rust-lang.org/cargo/reference/publishing.html
[4]: https://github.com/ChainSafe/forest/releases/new
[5]: https://github.com/ChainSafe/forest/blob/main/CHANGELOG.md
[6]: https://github.com/ChainSafe/forest/actions/workflows/docker-latest-tag.yml
[7]: https://github.com/ChainSafe/forest/pkgs/container/forest
