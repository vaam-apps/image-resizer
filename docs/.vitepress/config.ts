import { defineMd3Config } from "@vaam-apps/vitepress-md3/config";

// Served at the root of its custom domain, emgr-oss.vaam.store, from the
// gh-pages branch it shares with the Helm chart repository (index.yaml).
// Set DOCS_BASE=/image-resizer/ to preview the github.io project path.
const base = process.env.DOCS_BASE ?? "/";
const repo = "https://github.com/vaam-apps/image-resizer";

export default defineMd3Config({
  base,
  lang: "en",
  title: "emgr",
  titleTemplate: ":title · emgr",
  description:
    "A high-performance, imgproxy-compatible image resizing service in pure Rust.",
  cleanUrls: true,
  lastUpdated: true,
  // A link to a page that does not exist fails the build.
  ignoreDeadLinks: false,
  sitemap: { hostname: "https://emgr-oss.vaam.store" },
  head: [["link", { rel: "icon", type: "image/svg+xml", href: `${base}logo.svg` }]],

  markdown: {
    // These pages quote Helm/Go templates (`{{ include ... }}`) in inline
    // code. VitePress compiles Markdown as Vue, so mark inline code v-pre
    // or the braces are evaluated as an interpolation and the build fails.
    config(md) {
      const render = md.renderer.rules.code_inline;
      md.renderer.rules.code_inline = (tokens, idx, options, env, self) => {
        const html = render
          ? render(tokens, idx, options, env, self)
          : `<code>${md.utils.escapeHtml(tokens[idx]?.content ?? "")}</code>`;
        return html.replace(/^<code/, "<code v-pre");
      };
    },
  },

  themeConfig: {
    md3: {
      style: "expressive",
      // Indigo, carried over from the MkDocs palette these docs used before.
      seed: "#3f51b5",
    },

    logo: "/logo.svg",

    nav: [
      { text: "Get started", link: "/getting-started/installation", activeMatch: "^/getting-started/" },
      { text: "API", link: "/user-guide/api-reference", activeMatch: "^/user-guide/" },
      { text: "Deploy", link: "/deployment/helm-chart", activeMatch: "^/deployment/" },
      {
        text: "More",
        items: [
          { text: "Architecture", link: "/architecture/overview" },
          { text: "Contributing", link: "/development/contributing" },
          { text: "Changelog", link: "/about/changelog" },
          { text: "All vaam-apps docs", link: "https://vaam-apps.github.io/vitepress-md3/projects/" },
        ],
      },
    ],

    sidebar: [
      {
        text: "Getting started",
        items: [
          { text: "Introduction", link: "/" },
          { text: "Installation", link: "/getting-started/installation" },
          { text: "Configuration", link: "/getting-started/configuration" },
          { text: "Performance tuning", link: "/configuration/performance" },
        ],
      },
      {
        text: "User guide",
        items: [
          { text: "API reference", link: "/user-guide/api-reference" },
          { text: "Examples", link: "/user-guide/examples" },
        ],
      },
      {
        text: "Architecture",
        items: [
          { text: "Overview", link: "/architecture/overview" },
          { text: "Components", link: "/architecture/components" },
        ],
      },
      {
        text: "Deployment",
        items: [
          { text: "Helm chart", link: "/deployment/helm-chart" },
          { text: "Docker", link: "/deployment/docker" },
        ],
      },
      {
        text: "Development",
        items: [
          { text: "Contributing", link: "/development/contributing" },
          { text: "Testing", link: "/development/testing" },
        ],
      },
      {
        text: "About",
        items: [
          { text: "License", link: "/about/license" },
          { text: "Changelog", link: "/about/changelog" },
        ],
      },
    ],

    outline: { level: [2, 3] },
    search: { provider: "local" },
    socialLinks: [{ icon: "github", link: repo }],
    editLink: {
      pattern: `${repo}/edit/main/docs/:path`,
      text: "Edit this page on GitHub",
    },
    footer: {
      message: "Released under the MIT License.",
    },
  },
});
