// @ts-check
// Note: type annotations allow type checking and IDEs autocompletion

const lightCodeTheme = require("prism-react-renderer/themes/github");
const darkCodeTheme = require("prism-react-renderer/themes/dracula");

/** @type {import('@docusaurus/mdx-loader').MDXPlugin} */
// @ts-ignore
const mermaidPlugin = require("mdx-mermaid");

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: "Forest Docs",
  tagline: "Filecoin Rust Implementation",
  url: "https://forest.chainsafe.io",
  baseUrl: "/",
  onBrokenLinks: "throw",
  onBrokenMarkdownLinks: "throw",
  favicon: "img/logo.png",
  organizationName: "ChainSafe", // Usually your GitHub org/user name.
  projectName: "forest", // Usually your repo name.

  presets: [
    [
      "@metamask/docusaurus-openrpc/dist/preset",
      /** @type {import('@metamask/docusaurus-openrpc/dist/preset').Options} */
      ({
        blog: false,
        pages: false,
        docs: {
          id: "userDocs",
          routeBasePath: "/",
          path: "docs/users",
          sidebarPath: require.resolve("./userSidebars.js"),
          editUrl: "https://github.com/chainsafe/forest",
          remarkPlugins: [mermaidPlugin],
          openrpc: {
            openrpcDocument: "./docs/users/openrpc.json",
            path: "reference",
            sidebarLabel: "JSON-RPC",
          },
        },
        theme: {
          customCss: require.resolve("./src/css/index.css"),
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      colorMode: {
        defaultMode: "dark",
        disableSwitch: true,
        respectPrefersColorScheme: false,
      },
      navbar: {
        title: "Forest Docs",
        hideOnScroll: true,
        logo: {
          alt: "Forest Logo",
          src: "img/logo.png",
        },
        items: [
          {
            href: "https://github.com/chainsafe/forest",
            label: "GitHub",
            position: "right",
          },
        ],
      },
      announcementBar: {
        id: "new_docs",
        content:
          '🌲 Welcome to our new docs! Let us know what you think with <a target="_blank" rel="noopener noreferrer" href="https://forms.gle/zvfuJ9JF2V2qCFzHA">this form</a> 👋',
        backgroundColor: "#000000",
        textColor: "var(--ifm-color-primary)",
        isCloseable: false,
      },
      docs: {
        sidebar: {
          hideable: true,
        },
      },
      footer: {
        style: "dark",
        links: [],
        copyright: `Copyright © ${new Date().getFullYear()} ChainSafe. Built with Docusaurus.`,
      },
      prism: {
        theme: lightCodeTheme,
        darkTheme: darkCodeTheme,
      },
    }),
  plugins: [
    [
      "@docusaurus/plugin-content-docs",
      {
        id: "devDocs",
        routeBasePath: "developers",
        path: "docs/developers",
        sidebarPath: require.resolve("./devSidebars.js"),
        editUrl: "https://github.com/chainsafe/forest",
        remarkPlugins: [mermaidPlugin],
        showLastUpdateTime: true,
        showLastUpdateAuthor: true,
      },
    ],
  ],
  markdown: {
    mermaid: true,
  },
  themes: [
    [
      "@easyops-cn/docusaurus-search-local",
      /** @type {import("@easyops-cn/docusaurus-search-local").PluginOptions} */
      {
        hashed: true,
        highlightSearchTermsOnTargetPage: true,
        docsRouteBasePath: ["/", "developers"],
        docsDir: ["docs/users", "docs/developers"],
        docsPluginIdForPreferredVersion: "userDocs",
        searchContextByPaths: [
          {
            label: "Users",
            path: "/",
          },
          {
            label: "Devs",
            path: "developers",
          },
        ],
      },
    ],
    ["@docusaurus/theme-mermaid", {}],
  ],
};

module.exports = config;
