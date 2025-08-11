# Motivation
This document aims to provide developers with some guidelines for developing quality rust applications.

## Project structure
This repository uses `cargo workspaces` to manage rust projects. There are two important implications for this when creating and managing rust projects.

1. All dependencies are documented in the root `Cargo.toml`
  i. All dependencies should have `default-features` set to `false`
  ii. Project specific dependencies should be added in the `<path/to-project/Cargo.toml`
2. Any cargo command can be applied to an individual package via the `-p <package-name>` flag

### New subprojects: library or binary
All new rust projects should fall under the `/packages` or `/programs` directory. The former should contain reusable, and often but not necessarily complex, units of code. The latter should contain executable binary programs.

There are different code style guidelines for libraries and programs. These are explored below.

## General rust guidelines
In general we want to write rust code that makes use of:
- The type system
- Exhaustative matching patterns
- Asynchronous parallelisation (Futures)

Things we should not be pedantic about:
- Puritan functional programming features
- Forcing rust idioms where traditional approaches may be just as readable
- Multithreading things with channels

### Examples
#### Types for opaque but different things
We can use the type system to make a certain class of bugs impossible:
```rust
fn validate(proof: Vec<u8>, value: Vec<u8>) -> bool {
  proof.len() > value.len()
}

let proof = vec![1, 2, 3];
let value = vec![1, 2];

// Nope, value and proof are the wrong way round
assert!(validate(value, proof));
```
If we refactor using newtypes, this bug is caught at compile time:
```rust
struct Proof(Vec<u8>);

struct Value(Vec<u8>);

fn validate(proof: Proof, value: Value) -> bool {
  proof.0.len() > value.0.len()
}

let proof = Proof(vec![1, 2, 3]);
let value = Value(vec![1, 2]);

// Won't even compile
assert!(validate(value, proof));
```
#### Enums are the compiler's favourite
We can use enums in many different ways in rust. Without a doubt they are the safest way to express invariance in your programs behaviour. In this example we follow a similar (allbeit much simpler) approach to `alloy` to have a type safe request builder.

Imagine how error prone this code would be if `kind` was simply `String`
```rust

enum BlockKind {
  Latest,
  Finalized,
  Number(u64),
}

async fn get_block(client: &RpcClient, kind: &BlockKind) -> Result<Block, RpcError> {
  let request_params = match kind {
      BlockKind::Latest => "latest".to_string(),
      BlockKind::Finalized => "finalized".to_string(),
      BlockKind::Number(n) => n.to_hex(),
  };

  client.send([request_params]).await
}
```

#### Results are enums too
Using match means we have compiler safety around our error handling
```rust
let attestor = Attestor::new();

match client.get_block(&client, &BlockKind::Latest) {
  Ok(block) => attestor.sign(block),
  Err(e) => attestor.sign_nothing()
}
```
#### To for or for_each
We shouldn't care too much about using rust's functional methods like `for_each` instead of a classic loop.


#### Match can also be overkill
Sometimes a good old if statement is just way more expressive:
```rust
let key_too_long = pubkey.len() > 33;

match key_too_long {
  true => Err(...),
  false => Ok(pubkey),
}

// OR 

if key_too_long {
  return Err(...);
}
Ok(pubkey)
```



## Software principles
- Use existing project structure, modules or packages and programs, as a guideline for your code structure:
    - Try and keep binary program code to minimum and use libraries/packages instead
    - Store all project deps in the root `Cargo.toml`
- It's acceptable for binary projects to use `dyn` Errors instead of an exhaustative Error enum
- We think Newtyping for type safety is a valuable pattern, especially for novel software.
- If our programes implements existing specs written in different languages like Ethereum then it's often safer to copy the spec one to one
- When writing tests, if you feel the need to write mock out network calls or other services, this is an indicator that you should use a testing crate
- Anything more than a unit test should be located the `<package-name>/test` directory so that `cargo test` interprets it as in integration test.
- Double validation between services is important and when possible should be implemented using newtypes











