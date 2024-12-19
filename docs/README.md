# Forest Documentation

This directory contains a Docusaurus documentation website for both user and developer documentation.

## Overview
The site it comprised of two sub-sites - user documentation (`/`) and developer documentation (`/developers`). There is intentionally no link from the user docs to the developer docs, to avoid overwhelming users with unnecessary info.

### User Docs

Available at the root (`/`), default location for anyone visiting the documentation domain.

Follows the [Diátaxis](https://diataxis.fr/) framework for structuring documentation. The site is divided into four types of documentation: tutorials (Getting Started), how-to guides (Guides), explanations (Knowledge Base) and reference (Reference).

#### CLI Docs
These docs are automatically generated from the Forest CLI. See [script](/docs/docs/users/reference/cli.sh).

#### JSON-RPC Docs
We use the OpenRPC document from Forest to populate the documentation for each method. For this we use `@metamask/docusaurus-openrpc`.

### Developer Docs

Available at `/developers`. Comprised of a collection of documents aimed at contributors. May be relevant to power users.

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

## Deployment

The documentation site is continuously deployed to CloudFlare Pages, triggered on every commit to `main`.  [This workflow](/.github/workflows/docs-deploy.yml) defines the deployment process.