# ecma-parse-cat

ECMAScript parser consuming [ecma-lex-cat](https://crates.io/crates/ecma-lex-cat) tokens and producing [ecma-syntax-cat](https://crates.io/crates/ecma-syntax-cat) `Program` ASTs.

The third layer of a multi-crate comp-cat-rs reformulation of a JavaScript engine targeting Tauri integration.

## Coverage

- All statement forms (block, if, switch, for / for-in / for-of / for-await, while, do-while, return, throw, try/catch/finally, break, continue, labeled, expression, empty, debugger).
- All declaration forms (var/let/const, function, class).
- Classes: constructor, methods, getters, setters, public fields, private fields, static blocks.
- Modules: default/named/namespace imports, named/default/declaration/re-export-all exports.
- All expression operators with proper precedence climbing.
- Arrow functions via paren lookahead (`x => x`, `(x) => x`, `(x, y) => x + y`, `({ a, b }) => a`).
- Destructuring patterns (array, object, rest, default).
- Template literals with interpolation.
- Optional chaining (`?.`) and nullish coalescing (`??`).
- async/await/yield (not context-checked v0).

ASI is partial: semicolons are optional before `}` and EOF; required elsewhere.  Full newline-based ASI is a v0.2 TODO.

## Usage

```rust
# fn main() -> Result<(), ecma_parse_cat::error::Error> {
use ecma_parse_cat::parse_script;

let program = parse_script("let x = 1 + 2;")?;
println!("{program}");
# Ok(())
# }
```

## Building

```sh
cargo build
cargo test
RUSTFLAGS="-D warnings" cargo clippy --all-targets
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
