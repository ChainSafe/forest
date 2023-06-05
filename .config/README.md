# Spellcheck

We run spellchecks using
[`cargo-spellcheck`](https://crates.io/crates/cargo-spellcheck).

It delegates to a program called
[`Hunspell`](https://github.com/hunspell/hunspell).

Hunspell accepts uses `dictionary` files for words and `affix` files to define
acceptable modifications to those words.

Note that cargo-spellcheck comes with
[default dictionary and affix files](https://github.com/drahnr/cargo-spellcheck/blob/dff48db8ca954fce14a0cd5aea127ce59a929624/src/checker/hunspell.rs#L32).
Our vendored `en_US.dic` is larger than theirs.

See `forest.dic` in this directory:

```dic
syscall/M
```

`syscall` is the word, and `/M` applies the `M` flag in the
[affix file](https://github.com/drahnr/cargo-spellcheck/blob/dff48db8ca954fce14a0cd5aea127ce59a929624/hunspell-data/en_US.aff#L103):

```aff
SFX M   0     's         .
```

In this case, `'s` and `s` are acceptable suffixes for `syscall`, so the
following are allowed:

- `syscall`
- `syscalls`
- `syscall's`

For more information see
[the `Hunspell` manual](https://manpages.ubuntu.com/manpages/bionic/man5/hunspell.5.html)

## Tips

- Include symbols in `backticks` - they won't have to be added to the dictionary
