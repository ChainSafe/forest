<!-- omit in toc -->

# Contributing to Forest

First off, thanks for taking the time to contribute! ❤️ 🌲

All types of contributions are encouraged and valued. See the
[Table of Contents](#table-of-contents) for different ways to help and details
about how this project handles them. Please make sure to read the relevant
section before making your contribution. It will make it a lot easier for us
maintainers and smooth out the experience for all involved. The community looks
forward to your contributions. 🎉

> And if you like the project, but just don't have time to contribute, that's
> fine. There are other easy ways to support the project and show your
> appreciation, which we would also be very happy about:
>
> - Star the project
> - Tweet about it
> - Refer this project in your project's readme
> - Mention it to your Filecoin/IPFS/Rust friends

<!-- omit in toc -->

## Table of Contents

- [I Have a Question](#i-have-a-question)
- [I Want To Contribute](#i-want-to-contribute)
- [Reporting Bugs](#reporting-bugs)
- [Suggesting Enhancements](#suggesting-enhancements)
- [Your First Code Contribution](#your-first-code-contribution)
- [Improving The Documentation](#improving-the-documentation)
- [Styleguides](#styleguides)
- [Commit Messages](#commit-messages)
- [Join The Project Team](#join-the-project-team)

## I Have a Question

> If you want to ask a question, we assume that you have read the available
> [Documentation](https://docs.forest.chainsafe.io/).

Before you ask a question, it is best to search for existing
[Issues](https://github.com/ChainSafe/forest/issues) or
[Q&A](https://github.com/ChainSafe/forest/discussions/categories/forest-q-a)
that might help you. In case you have found a suitable issue and still need
clarification, you can write your question in this issue. It is also advisable
to search the internet for answers first.

If you then still feel the need to ask a question and need clarification, we
recommend the following:

- Ask a question in the
  [Forest Q&A](https://github.com/ChainSafe/forest/discussions/categories/forest-q-a)
  section.
- Provide as much context as you can about what you're running into.
- Provide project and platform versions, depending on what seems relevant.

We will then take care of the issue as soon as possible.

You can also ask us questions on the [Filecoin Slack](https://filecoin.io/slack)
on the `#fil-forest-help` channel.

## I Want To Contribute

> ### 🏛️ Legal Notice <!-- omit in toc -->
>
> When contributing to this project, you must agree that you have authored 100%
> of the content, that you have the necessary rights to the content and that the
> content you contribute may be provided under the project license.

### Reporting Bugs

<!-- omit in toc -->

#### 👾 Before Submitting a Bug Report

A good bug report shouldn't leave others needing to chase you up for more
information. Therefore, we ask you to investigate carefully, collect information
and describe the issue in detail in your report. Please complete the following
steps in advance to help us fix any potential bug as fast as possible.

- Make sure that you are using the latest version.
- Determine if your bug is really a bug and not an error on your side e.g. using
  incompatible environment components/versions (Make sure that you have read the
  [documentation](https://docs.forest.chainsafe.io/). If you are looking for
  support, you might want to check [this section](#i-have-a-question)).
- To see if other users have experienced (and potentially already solved) the
  same issue you are having, check if there is not already a bug report existing
  for your bug or error in the
  [bug tracker](https://github.com/ChainSafe/forest/issues?q=label%3A"Type%3A+Bug").
- Also make sure to search the internet (including Stack Overflow) to see if
  users outside of the GitHub community have discussed the issue.
- Collect information about the bug:
- Stack trace (Traceback)
- OS, Platform and Version (Windows, Linux, macOS, x86, ARM)
- Version of the interpreter, compiler, SDK, runtime environment, package
  manager, depending on what seems relevant.
- Possibly your input and the output
- Can you reliably reproduce the issue? And can you also reproduce it with older
  versions?

<!-- omit in toc -->

#### 👾 How Do I Submit a Good Bug Report?

> You must never report security related issues, vulnerabilities or bugs
> including sensitive information to the issue tracker, or elsewhere in public.
> Instead sensitive bugs must be sent by email to .

<!-- You may add a PGP key to allow the messages to be sent encrypted as well. -->

We use GitHub issues to track bugs and errors. If you run into an issue with the
project:

- Open an
  [bug report](https://github.com/ChainSafe/forest/issues/new?assignees=&labels=Type%3A+Bug&projects=&template=1-bug_report.md&title=).
- Explain the behavior you would expect and the actual behavior.
- Please provide as much context as possible and describe the _reproduction
  steps_ that someone else can follow to recreate the issue on their own. This
  usually includes your code. For good bug reports you should isolate the
  problem and create a reduced test case.
- Provide the information you collected in the previous section.

Once it's filed:

- A team member will try to reproduce the issue with your provided steps. If
  there are no reproduction steps or no obvious way to reproduce the issue, the
  team will ask you for those steps and mark the issue as `needs-repro`. Bugs
  with the `needs-repro` tag will not be addressed until they are reproduced.
- If the team is able to reproduce the issue, it will be marked `needs-fix`, as
  well as possibly other tags, and the issue will be left to be
  [implemented by someone](#your-first-code-contribution).

### Suggesting Enhancements

This section guides you through submitting an enhancement suggestion for Forest,
**including completely new features and minor improvements to existing
functionality**. Following these guidelines will help maintainers and the
community to understand your suggestion and find related suggestions.

<!-- omit in toc -->

#### 🎯 Before Submitting an Enhancement

- Make sure that you are using the latest version.
- Read the [documentation](https://docs.forest.chainsafe.io/) carefully and find
  out if the functionality is already covered, maybe by an individual
  configuration.
- Perform a [search](https://github.com/ChainSafe/forest/issues) to see if the
  enhancement has already been suggested. If it has, add a comment to the
  existing issue instead of opening a new one.
- Find out whether your idea fits with the scope and aims of the project. It's
  up to you to make a strong case to convince the project's developers of the
  merits of this feature. Keep in mind that we prioritize features that will be
  useful to the majority of our users and not just a small subset.

<!-- omit in toc -->

#### 🎯 How Do I Submit a Good Enhancement Suggestion?

Enhancement suggestions are tracked with the `Type: Request` label. Please use
[this template](https://github.com/ChainSafe/forest/issues/new?assignees=&labels=Type%3A+Request&projects=&template=2-user_request.md&title=).

- Use a **clear and descriptive title** for the issue to identify the
  suggestion.
- Provide a **step-by-step description of the suggested enhancement** in as many
  details as possible.
- **Describe the current behavior** and **explain which behavior you expected to
  see instead** and why. At this point you can also tell which alternatives do
  not work for you.
- **Explain why this enhancement would be useful** to most Forest users. You may
  also want to point out the other projects that solved it better and which
  could serve as inspiration.

### Your First Code Contribution

#### 🛠️ Install the tools

Forest is mostly written in Rust, so you will need to have the Rust toolchain
installed as normal. The version is specified in the
[./rust-toolchain.toml](./rust-toolchain.toml) file. You can install the Rust
toolchain by following the instructions on the
[Rust website](https://www.rust-lang.org/tools/install).

You will also need to install Go - the toolchain version is specified in the
[./go.work](./go.work) file. You can install Go by following the instructions on
the [Go website](https://golang.org/doc/install).

We also use linters and tools to work with the code - you can install them by
running `make install-lint-tools`.

#### 👥Fork and clone the repository

To contribute to Forest, you will need to fork the repository and clone it to
your local machine. You can read more in the
[GitHub documentation](https://docs.github.com/en/get-started/exploring-projects-on-github/contributing-to-a-project).

#### ✅Check that everything works

Before you start making changes, you should make sure that everything works. You
can do this by running the tests with `make test`. Note that you need to have
[cargo nextest](https://nexte.st/) installed to run the tests.

#### 💻Make your changes

Now you can make your changes! Make sure to follow the
[styleguides](#styleguides) and run the linters before submitting your changes.

#### 🚀Submit your changes

When you are ready to submit your changes, you can open a pull request. Make
sure to fill exhaustively the PR template and provide as much context as
possible. You can read more about opening a pull request in the
[GitHub documentation](https://docs.github.com/en/github/collaborating-with-issues-and-pull-requests/creating-a-pull-request).

If you are a first-time contributor to the project, you will need to sign the
Contributor License Agreement, which will be prompted when you open your pull
request.

### 🌲Enjoy contributing

Congratulations! You have successfully contributed to Forest. 🎉 We are
eternally grateful and hope you will continue to contribute to the project.

### 📚 Improving The Documentation

The documentation is currently hosted on
[docs.forest.chainsafe.io](https://docs.forest.chainsafe.io/). If you find any
issues with the documentation, please create an issue or contribute to the [docs directory](docs).

## Styleguides

### 📝 Documentation practices

Code documentation is expected to be present in all code files, especially for
public functions and structs. Please refer to the Forest team's
[Documentation practices](https://github.com/ChainSafe/forest/wiki/Documentation-practices).

### 🤖Code formatting

Formatting is standardised via various formatting tools for different
technologies. Please make sure to run the appropriate formatter before
submitting your code, otherwise it will not pass the CI checks. You can format
the code, including markdown files, with `make fmt`.

### 💬 Commit Messages

We aim to use
[conventional commits](https://www.conventionalcommits.org/en/v1.0.0/) for our
commit messages. This allows for better readability and changelog generation.

Example of a good commit message:

```
feat: add `Filecoin.RuleTheWorld` RPC method
```

Example of a bad commit message:

```
fixed bug
```

<!-- omit in toc -->

## Attribution

This guide is based on the **contributing-gen**.
[Make your own](https://github.com/bttger/contributing-gen)!
