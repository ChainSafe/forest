# Spellcheck

We run spellchecks using
[`cargo-spellcheck`](https://crates.io/crates/cargo-spellcheck) with the
[Hunspell](https://github.com/hunspell/hunspell) backend.

This is used for **both** Rust doc comments and Markdown files across the entire
repository. A single dictionary file (`forest.dic`) is shared between all
checks.

Hunspell uses `dictionary` files for words and `affix` files to define
acceptable modifications to those words.

Note that `cargo-spellcheck` comes with
[default dictionary and affix files](https://github.com/drahnr/cargo-spellcheck/blob/dff48db8ca954fce14a0cd5aea127ce59a929624/src/checker/hunspell.rs#L32).
Our vendored `en_US.dic` is larger than theirs.

## How it works

See `forest.dic` in this directory:

```dic
Filecoin/M
```

`Filecoin` is the word, and `/M` applies the `M` flag in the
[affix file](https://github.com/drahnr/cargo-spellcheck/blob/dff48db8ca954fce14a0cd5aea127ce59a929624/hunspell-data/en_US.aff#L103):

```aff
SFX M   0     's         .
```

In this case, `'s` and `s` are acceptable suffixes, so the following are
allowed:

- `Filecoin`
- `Filecoins`
- `Filecoin's`

As another example, take the following entry:

```dic
syscall/S
```

Where the `S` flag is
[as follows](https://github.com/drahnr/cargo-spellcheck/blob/dff48db8ca954fce14a0cd5aea127ce59a929624/hunspell-data/en_US.aff#L91-L95):

<!-- Use a block instead of inline code to stop the spacing being murdered by 'pretter' -->

```c
// Define a suffix, called S, allow mixing prefixes and suffixes, with 4 rules.
SFX S Y 4
// Remove a trailing `y`, replace it with `ies`, if the word ends in a `y` not preceded by a vowel.
SFX S   y     ies        [^aeiou]y
// Don't remove any trailing characters, add an s, if the word ends in a `y` preceded by a vowel.
SFX S   0     s          [aeiou]y
SFX S   0     es         [sxzh]
SFX S   0     s          [^sxzhy]
```

So the following would be allowed:

- `syscall`
- `syscalls`

Flags may be combined - you will often see `/SM`, for example.

For more information see
[the `Hunspell` manual](https://manpages.ubuntu.com/manpages/bionic/man5/hunspell.5.html)

## Tips

- Include symbols in `backticks` - they won't have to be added to the dictionary
- Wrap code identifiers (struct names, variable names, crate names) in backticks
  rather than adding them to the dictionary
- Only add common IT terms, proper nouns, and domain-specific terminology to the
  dictionary
- Use `<URL>` autolink syntax for bare URLs in Markdown files so they are skipped
  by the checker
- Run `mise run lint:spellcheck` for Rust code and
  `mise run lint:spellcheck-markdown` for Markdown files
