# Contributing to Matching Engine

Thank you for your interest in contributing! This document provides guidelines for development.

## Development Setup

### Prerequisites

- Rust 1.93.0 or later (see `rust-toolchain.toml`)
- Cargo
- Git
- Python 3.x (for pre-commit hooks)

### Initial Setup

```bash
# Clone the repository
git clone https://github.com/trpleo/matching-engine.git
cd matching-engine

# Install pre-commit hooks
pip install pre-commit
pre-commit install
pre-commit install --hook-type commit-msg

# Build the project
cargo build --all-features

# Run tests
cargo test --all-features

# Verify setup
cargo clippy --all-features
```

## Code Quality

### Formatting

We use `rustfmt` for code formatting. The configuration is in `rustfmt.toml`.

**Format your code before committing:**

```bash
# Format all code
make fmt

# Or directly with cargo
cargo fmt --all
```

**Check if code is formatted:**

```bash
make fmt-check
```

**Configuration:**
- Line width: 100 characters
- 4 spaces for indentation
- Unix line endings
- Reorder imports automatically

### Linting

We use `clippy` for linting:

```bash
# Run clippy
make clippy

# Run clippy with pedantic warnings
make clippy-pedantic

# Auto-fix issues
make fix
```

### Pre-commit Checklist

Before submitting a PR, ensure:

```bash
# Run all checks
make pre-commit
```

This runs:
1. `cargo fmt` - Format code
2. `cargo clippy` - Lint code
3. `cargo test` - Run all tests

## Testing

### Running Tests

```bash
# All tests
make test

# Unit tests only
make test-unit

# Integration tests
make test-int

# With verbose output
make test-verbose
```

### Writing Tests

- **Unit tests**: Add `#[cfg(test)]` module in the same file
- **Integration tests**: Add files in `tests/` directory
- **Benchmarks**: Add to `benches/matching_benchmark.rs`

Example unit test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_creation() {
        let order = Order::new(
            "user1".to_string(),
            "BTC-USD".to_string(),
            Side::Buy,
            OrderType::Limit,
            Some(Decimal::from(50000)),
            Decimal::from(1),
            TimeInForce::GoodTillCancel,
        );

        assert_eq!(order.get_remaining_quantity(), Decimal::from(1));
    }
}
```

## Benchmarking

Run benchmarks with:

```bash
make bench
```

Benchmark results are saved in `target/criterion/`.

## Documentation

### Generating Documentation

```bash
# Generate docs
make doc

# Generate and open in browser
make doc-open
```

### Documentation Guidelines

- Use `///` for public API documentation
- Use `//!` for module-level documentation
- Include examples in doc comments
- Document panics, errors, and safety requirements

Example:

```rust
/// Submits an order to the matching engine.
///
/// # Arguments
///
/// * `order` - The order to submit
///
/// # Returns
///
/// Vector of events generated during order processing
///
/// # Examples
///
/// ```
/// let order = Arc::new(Order::new(/*...*/));
/// let events = engine.submit_order(order);
/// ```
pub fn submit_order(&self, order: Arc<Order>) -> Vec<OrderEvent> {
    // ...
}
```

## Project Structure

```
src/
├── domain/          # Pure domain models
├── interfaces/      # Trait definitions
├── engine/          # Business logic
├── simd/            # Performance optimizations
└── utils/           # Utilities
```

### Module Guidelines

- **domain/**: No external dependencies, pure business logic
- **interfaces/**: Define contracts, no implementations
- **engine/**: Implement business logic using traits
- **simd/**: Platform-specific optimizations

## Branch Naming Convention

Create branches following this pattern:

- `feature/description` - New features
- `bugfix/description` - Bug fixes
- `hotfix/description` - Urgent production fixes
- `release/x.y.z` - Release preparation

**Examples:**
- `feature/add-market-orders`
- `bugfix/fix-price-overflow`
- `hotfix/security-patch`
- `release/1.0.0`

## Pull Request Process

1. **Fork** the repository
2. **Create** a feature branch from `develop` (`git checkout -b feature/amazing-feature`)
3. **Commit** your changes using [Conventional Commits](#commit-message-guidelines)
4. **Ensure all checks pass:**
   ```bash
   cargo fmt --check
   cargo clippy --all-features -- -D warnings
   cargo test --all-features
   ```
5. **Push** to the branch (`git push origin feature/amazing-feature`)
6. **Open** a Pull Request against `develop`
7. **Fill out** the PR template completely

### PR Requirements

- All tests must pass
- Code must be formatted (`cargo fmt`)
- No clippy warnings
- Documentation for new public APIs
- Add tests for new functionality

## Commit Message Guidelines

Follow conventional commits:

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `style`: Formatting
- `refactor`: Code refactoring
- `test`: Adding tests
- `chore`: Maintenance

**Examples:**

```
feat(engine): add Size Pro-Rata matching algorithm

Implemented size-based pro-rata allocation for derivatives
exchanges. Orders are allocated proportionally based on their
size at each price level.

Closes #123
```

```
fix(pro_rata): prevent infinite loop in calculate_allocation

Fixed bug where small orders below minimum quantity were
causing infinite loop by being repeatedly popped and pushed
back to the queue.
```

## Code Style

### Rust Conventions

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use descriptive variable names
- Prefer immutability
- Use type inference where possible
- Document public APIs

### Naming Conventions

- Types: `PascalCase`
- Functions/methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Modules: `snake_case`

### Error Handling

- Use `Result<T, E>` for recoverable errors
- Use `Option<T>` for optional values
- Document when functions panic
- Provide meaningful error messages

## Performance Considerations

- Use profiling before optimizing
- Benchmark critical paths
- Prefer zero-cost abstractions
- Document performance characteristics in comments

## Getting Help

- Open an issue for bugs or feature requests
- Join our discussions for questions
- Check existing issues before creating new ones

## License

This project is licensed under the PolyForm Noncommercial License 1.0.0. By contributing, you agree that your contributions will be licensed under the same terms.

**Note:** Commercial use requires a separate license agreement. See [LICENSE](LICENSE) for details.
