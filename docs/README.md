# Forest Documentation

This directory contains a Docusaurus documentation website for both user and developer documentation.

## Getting Started

> Note: This project uses [Yarn](https://yarnpkg.com/getting-started/install)

### Installation

Install the required dependencies:
```
$ yarn
```

### Local Development
Start local development server:
```
$ yarn start
```

### CI Checks

These commands are recommended to run before commiting code. They run as checks in CI.
```
yarn spellcheck # Checks spelling
yarn format     # Run prettier to fix formatting issues
yarn typecheck  # Validate typescript files
```
> **How to Fix Spellcheck Errors:** You can add unknown words to `dictionary.txt`.

### Build
To compile an optimized production build: 
```
$ yarn build
```

