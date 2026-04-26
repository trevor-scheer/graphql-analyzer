# @graphql-analyzer/core

Low-level [napi-rs][napi] binding that exposes the [graphql-analyzer] Rust core
to Node.js. This package is **not intended for direct consumption** — it's the
substrate for higher-level wrappers like
[`@graphql-analyzer/eslint-plugin`](https://www.npmjs.com/package/@graphql-analyzer/eslint-plugin).

The `.node` binary is distributed via platform-specific
`optionalDependencies` (`@graphql-analyzer/core-<triple>`); npm picks the
right one for your machine at install time.

For project documentation, see <https://trevor-scheer.github.io/graphql-analyzer/>.

## License

MIT OR Apache-2.0

[graphql-analyzer]: https://github.com/trevor-scheer/graphql-analyzer
[napi]: https://napi.rs
