# service-rs - An Async Dependency Injection Container for Rust

[<img alt="github" src="https://img.shields.io/badge/github-SFINXVC/service--rs-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/SFINXVC/service-rs)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/SFINXVC/service-rs/rust.yml?branch=main&style=for-the-badge" height="20">](https://github.com/SFINXVC/service-rs/actions?query=branch%3Amain)

An async-first, lightweight dependency injection (DI) container for Rust, inspired by [Microsoft.Extensions.DependencyInjection](https://learn.microsoft.com/en-us/dotnet/api/microsoft.extensions.dependencyinjection) from .NET.

## Installation

```toml
[dependencies]
service-rs = { git = "https://github.com/SFINXVC/service-rs.git", branch = "main" }
```

**Compiler support:** requires rustc 1.85.0 or higher (uses unstable features: `unsize`, `coerce_unsized`)

**Note:** This library is still in development and is not ready for production use.

## Features

- **Three service lifetimes:** Singleton, Scoped, and Transient
- **Async-first design:** All service resolution is async using `tokio`
- **Thread-safe:** Services are wrapped in `Arc<T>` for safe sharing across threads
- **Automatic dependency injection:** Use the `#[derive(Injectable)]` macro for automatic constructor injection
- **Trait object support:** Register implementations for trait objects with interface methods
- **Scoped services:** Create service scopes with scoped lifetime management

## Service Lifetimes

- **Singleton:** One instance created and shared across the entire application
- **Scoped:** One instance per scope; same instance within a scope, new instance for each scope
- **Transient:** New instance created every time the service is requested

## Basic Example

```rust
use service_rs::ServiceCollection;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let collection = ServiceCollection::new()
        .add_singleton_with_factory::<i32, _, _>(|_| async {
            Ok(Box::new(42) as Box<dyn std::any::Any + Send + Sync>)
        })
        .add_transient_with_factory::<String, _, _>(|_| async {
            Ok(Box::new("Hello".to_string()) as Box<dyn std::any::Any + Send + Sync>)
        });

    let provider = collection.build();

    // Singleton - same instance every time
    let num1: Arc<i32> = provider.get::<i32>().await.unwrap();
    let num2: Arc<i32> = provider.get::<i32>().await.unwrap();
    assert_eq!(Arc::as_ptr(&num1), Arc::as_ptr(&num2));

    // Transient - different instance every time
    let str1: Arc<String> = provider.get::<String>().await.unwrap();
    let str2: Arc<String> = provider.get::<String>().await.unwrap();
    assert_ne!(Arc::as_ptr(&str1), Arc::as_ptr(&str2));
}
```

## Using the Injectable Derive Macro

The `Injectable` derive macro enables automatic dependency injection for structs with dependencies.

```rust
use service_rs::{Injectable, ServiceCollection};
use std::sync::Arc;

struct Database {
    connection_string: String,
}

#[derive(Injectable)]
struct UserService {
    db: Arc<Database>,
}

#[tokio::main]
async fn main() {
    let collection = ServiceCollection::new()
        .add_singleton_with_factory::<Database, _, _>(|_| async {
            Ok(Box::new(Database {
                connection_string: "localhost:5432".to_string(),
            }) as Box<dyn std::any::Any + Send + Sync>)
        })
        .add_scoped::<UserService>();

    let provider = collection.build();
    let scope = provider.create_scope();

    let user_service: Arc<UserService> = scope.get::<UserService>().await.unwrap();
    // UserService automatically receives the Database dependency
}
```

**Important:** All fields in an `Injectable` struct must be wrapped in `Arc<T>`.

## Scoped Services Example

```rust
use service_rs::ServiceCollection;
use std::sync::Arc;

struct RequestContext {
    request_id: String,
}

#[tokio::main]
async fn main() {
    let collection = ServiceCollection::new()
        .add_scoped_with_factory::<RequestContext, _, _>(|_| async {
            Ok(Box::new(RequestContext {
                request_id: uuid::Uuid::new_v4().to_string(),
            }) as Box<dyn std::any::Any + Send + Sync>)
        });

    let provider = collection.build();

    // First scope
    let scope1 = provider.create_scope();
    let ctx1a: Arc<RequestContext> = scope1.get::<RequestContext>().await.unwrap();
    let ctx1b: Arc<RequestContext> = scope1.get::<RequestContext>().await.unwrap();
    assert_eq!(Arc::as_ptr(&ctx1a), Arc::as_ptr(&ctx1b)); // Same within scope

    // Second scope
    let scope2 = provider.create_scope();
    let ctx2: Arc<RequestContext> = scope2.get::<RequestContext>().await.unwrap();
    assert_ne!(Arc::as_ptr(&ctx1a), Arc::as_ptr(&ctx2)); // Different across scopes
}
```

## Trait Object Registration

Register implementations for trait objects using the interface methods:

```rust
use service_rs::{Injectable, ServiceCollection};
use std::sync::Arc;

trait Logger: Send + Sync {
    fn log(&self, message: &str);
}

#[derive(Injectable)]
struct ConsoleLogger;

impl Logger for ConsoleLogger {
    fn log(&self, message: &str) {
        println!("{}", message);
    }
}

#[tokio::main]
async fn main() {
    let collection = ServiceCollection::new()
        .add_singleton_interface::<dyn Logger, ConsoleLogger>();

    let provider = collection.build();

    let logger: Arc<Box<dyn Logger>> = provider.get::<Box<dyn Logger>>().await.unwrap();
    logger.log("Hello from trait object!");
}
```

## API Overview

### ServiceCollection Methods

**Factory-based registration:**
- `add_singleton_with_factory<T, F, Fut>(factory: F) -> Self`
- `add_scoped_with_factory<T, F, Fut>(factory: F) -> Self`
- `add_transient_with_factory<T, F, Fut>(factory: F) -> Self`

**Injectable-based registration (requires `proc-macro` feature):**
- `add_singleton<T: Injectable>() -> Self`
- `add_scoped<T: Injectable>() -> Self`
- `add_transient<T: Injectable>() -> Self`

**Interface registration (requires `proc-macro` feature):**
- `add_singleton_interface<T: ?Sized, TImpl: Injectable>() -> Self`
- `add_scoped_interface<T: ?Sized, TImpl: Injectable>() -> Self`
- `add_transient_interface<T: ?Sized, TImpl: Injectable>() -> Self`

**Build:**
- `build(self) -> Arc<ServiceProvider>`

### ServiceProvider Methods

- `async fn get<T>(&self) -> Result<Arc<T>, ServiceError>`
- `fn create_scope(&self) -> Arc<ScopedServiceProvider>`

### ScopedServiceProvider Methods

- `async fn get<T>(&self) -> Result<Arc<T>, ServiceError>`

## Error Handling

The library provides detailed error types via `ServiceError`:

- `ServiceNotFound` - Service type not registered
- `ServiceAlreadyExists` - Service type already registered
- `ServiceResolutionFailed` - Failed to resolve service
- `ServiceInitializationFailed` - Factory threw an error
- `ServiceInvalidScope` - Attempted to resolve scoped service from root provider

## Features

- `proc-macro` (default): Enables the `Injectable` derive macro and interface registration methods

## License

This project is licensed under the MIT License.
