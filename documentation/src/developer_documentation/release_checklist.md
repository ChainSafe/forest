Forest follows a fixed, quarterly release schedule. On the last week of each
quarter, a new version is always released. This is supplemented with additional
releases for bug fixes and special features. A "release officer" is appointed
for each release and they are responsible for either following the checklist or,
in case of absence, passing the task to a different team member.

1. Update the CHANGELOG.md file to reflect all changes and preferably write a
   small summary about the most notable updates. The changelog should follow the
   design philosophy outlined here: https://keepachangelog.com/en/1.0.0/. Go
   through the output of `git log` and remember that the audience of the
   CHANGELOG does not have intimate knowledge of the Forest code-base. All the
   changed/updated/removed features should be reasonably understandable to an
   end-user.
2. Update that version of the crates that are to be released. Forest contains
   many crates so you may need to update many Cargo.toml files. If you're
   working on a patch release, make sure that there are no breaking changes.
   Cherry-picking of patches may be necessary.
3. Run the manual tests steps outlined in the TEST_PLAN.md. Caveat: Right now
   there are no manual test steps so this step can be skipped.
4. Once the changes in step 1 and step 2 have been merged, tag the commit with
   the new version number. The version tag should start with a lowercase 'v'.
   Example: v0.4.1
5. Publish the new crate on crates.io according to the
   [manual](https://doc.rust-lang.org/cargo/reference/publishing.html).
6. Go to https://github.com/ChainSafe/forest/releases/new and create a new
   release. Use the tag created in step 4, follow the title convention of the
   previous releases, and write a small summary of the release (similar or
   identical to the summary in the CHANGELOG.md file).
7. Verify that the new release contains assets for both Linux and MacOS (the
   assets are automatically generated and should show up after 30 minutes to an
   hour).
8. Verify that the new release is available in the Github Container Registry.
   Use `docker pull ghcr.io/chainsafe/forest:<version>` and ensure that it is
   present in the [packages][1]
9. Make sure the `Cargo.lock` change is included in the pull request.

[1]: https://github.com/ChainSafe/forest/pkgs/container/forest
