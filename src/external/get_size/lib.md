Determine the size in bytes an object occupies inside RAM.

The [`GetSize`] trait can be used to determine the size of an object inside the stack as well as in the heap. The [`size_of`](std::mem::size_of) function provided by the standard library can already be used to determine the size of an object in the stack, but many application (e.g. for caching) do also need to know the number of bytes occupied inside the heap, for which this library provides an appropriate trait.

#### Example

We use [`GetSize`] to determine number of bytes occupied by both a [`String`] and a [`Vec`] of bytes. Note that the [`Vec`] has already allocated a capacity of `1024` bytes, and does thus correctly show a heap size of `1024`, even if only `1` byte is currently in use.

```rust
use get_size::GetSize;

fn main() {
  let value = String::from("Hello World!");

  assert_eq!(String::get_stack_size(), std::mem::size_of::<String>());
  assert_eq!(value.get_heap_size(), 12);

  assert_eq!(value.get_size(), std::mem::size_of::<String>() + 12);


  let mut buffer = Vec::with_capacity(1024); // 1KB allocated on the heap.
  buffer.push(1u8); // 1 byte in use.

  assert_eq!(buffer.len(), 1);
  assert_eq!(buffer.get_heap_size(), 1024);
}
```

# Ownership based accounting

This library follows the idea that only bytes owned by a certain object should be accounted for, and not bytes owned by different objects which are only borrowed. This means in particular that objects referenced by pointers are ignored.

#### Example

```rust
use get_size::GetSize;

#[derive(GetSize)]
struct Test<'a> {
  value: &'a String,
}

fn main() {
  let value = String::from("hello");

  // This string occupies 5 bytes at the heap, but a pointer is treated as not occupying
  // anything at the heap.
  assert_eq!(value.get_heap_size(), 5);
  assert_eq!(GetSize::get_heap_size(&&value), 0); // Fully qualified syntax

  // WARNING: Duo to rust's automatic dereferencing, a simple pointer will be dereferenced
  // to the original value, causing the borrowed bytes to be accounted for too.
  assert_eq!((&value).get_heap_size(), 5);
  // The above gets rewritten by to compiler into:
  // assert_eq!(value.get_heap_size(), 5);

  // Our derive macro uses fully qualified syntax, so auto-dereferencing does
  // not occour.
  let value = Test {
    value: &value,
  };

  // The String is now only borrowed, leading to its heap bytes not being
  // accounted for.
  assert_eq!(value.get_heap_size(), 0);
}
```

On the other hand references implemented as shared ownership are treated as owned values. It is your responsibility to ensure that the bytes occupied by them are not counted twice in your application. The `ignore` attribute might be helpful, [see below](#ignoring-certain-values).

#### Example

```rust
use std::sync::Arc;
use get_size::GetSize;

fn main() {
  let value = String::from("hello");
  assert_eq!(value.get_heap_size(), 5);

  // From a technical point of view, Arcs own the data they reference.
  // Given so their heap data gets accounted for too.
  // Note that an Arc does store the String's stack bytes also inside the heap.
  let value = Arc::new(value);
  assert_eq!(value.get_heap_size(), std::mem::size_of::<String>() + 5);
}
```

# How to implement

The [`GetSize`] trait is already implemented for most objects defined by the standard library, like [`Vec`](std::vec::Vec), [`HashMap`](std::collections::HashMap), [`String`] as well as all the primitive values, like [`u8`], [`i32`] etc.

Unless you have a complex data structure which requires a manual implementation, you can easily derive [`GetSize`] for your own structs and enums. The derived implementation will implement [`GetSize::get_heap_size`] by simply calling [`GetSize::get_heap_size`] on all values contained inside the struct or enum variant and return the sum of them.

You will need to activate the `derive` feature first, which is disabled by default. Add the following to your `cargo.toml`:

```toml
get-size = { version = "^0.1", features = ["derive"] }
```

Note that the derive macro _does not support unions_. You have to manually implement it for them.

### Examples

Deriving [`GetSize`] for a struct:

```rust
use get_size::GetSize;

#[derive(GetSize)]
pub struct OwnStruct {
    value1: String,
    value2: u64,
}

fn main() {
    let test = OwnStruct {
        value1: "Hello".into(),
        value2: 123,
    };

    assert_eq!(test.get_heap_size(), 5);
}
```

Deriving [`GetSize`] for an enum:

```rust
use get_size::GetSize;

#[derive(GetSize)]
pub enum TestEnum {
    Variant1(u8, u16, u32),
    Variant2(String),
    Variant3,
    Variant4{x: String, y: String},
}

#[derive(GetSize)]
pub enum TestEnumNumber {
    Zero = 0,
    One = 1,
    Two = 2,
}

fn main() {
    let test = TestEnum::Variant1(1, 2, 3);
    assert_eq!(test.get_heap_size(), 0);

    let test = TestEnum::Variant2("Hello".into());
    assert_eq!(test.get_heap_size(), 5);

    let test = TestEnum::Variant3;
    assert_eq!(test.get_heap_size(), 0);

    let test = TestEnum::Variant4{x: "Hello".into(), y: "world".into()};
    assert_eq!(test.get_heap_size(), 5 + 5);

    let test = TestEnumNumber::One;
    assert_eq!(test.get_heap_size(), 0);
}
```

The derive macro does also work with generics. The generated trait implementation will by default require all generic types to implement [`GetSize`] themselves, but this [can be changed](#ignoring-certain-generic-types).

```rust
use get_size::GetSize;

#[derive(GetSize)]
struct TestStructGenerics<A, B> {
    value1: A,
    value2: B,
}

#[derive(GetSize)]
enum TestEnumGenerics<A, B> {
  Variant1(A),
  Variant2(B),
}

fn main() {
    let test: TestStructGenerics<String, u64> = TestStructGenerics {
        value1: "Hello".into(),
        value2: 123,
    };

    assert_eq!(test.get_heap_size(), 5);

    let test = String::from("Hello");
    let test: TestEnumGenerics<String, u64> = TestEnumGenerics::Variant1(test);

    assert_eq!(test.get_heap_size(), 5);

    let test: TestEnumGenerics<String, u64> = TestEnumGenerics::Variant2(100);

    assert_eq!(test.get_heap_size(), 0);
}
```

## Dealing with external types which do not implement GetSize

Deriving [`GetSize`] is straight forward if all the types contained in your data structure implement [`GetSize`] themselves, but this might not always be the case. For that reason the derive macro offers some helpers to assist you in that case.

Note that the helpers are currently only available for regular structs, that is they do neither support tuple structs nor enums.

### Ignoring certain values

You can tell the derive macro to ignore certain struct fields by adding the `ignore` attribute to them. The generated implementation of [`GetSize::get_heap_size`] will then simple skip this field.

#### Example

The idiomatic use case for this helper is if you use shared ownership and do not want your data to be counted twice.

```rust
use std::sync::Arc;
use get_size::GetSize;

#[derive(GetSize)]
struct PrimaryStore {
  id: u64,
  shared_data: Arc<Vec<u8>>,
}

#[derive(GetSize)]
struct SecondaryStore {
  id: u64,
  #[get_size(ignore)]
  shared_data: Arc<Vec<u8>>,
}

fn main() {
  let shared_data = Arc::new(Vec::with_capacity(1024));

  let primary_data = PrimaryStore {
    id: 1,
    shared_data: Arc::clone(&shared_data),
  };

  let secondary_data = SecondaryStore {
    id: 2,
    shared_data,
  };

  // Note that Arc does also store the Vec's stack data on the heap.
  assert_eq!(primary_data.get_heap_size(), Vec::<u8>::get_stack_size() + 1024);
  assert_eq!(secondary_data.get_heap_size(), 0);
}
```

#### Example

But you may also use this as a band aid, if a certain struct fields type does not implement [`GetSize`].

Be aware though that this will result in an implementation which will return incorrect results, unless the heap size of that type is indeed always zero and can thus be ignored. It is therefor advisable to use one of the next two helper options instead.

```rust
use get_size::GetSize;

// Does not implement GetSize!
struct TestStructNoGetSize {
    value: String,
}

// Implements GetSize, even through one field's type does not implement it.
#[derive(GetSize)]
struct TestStruct {
  name: String,
  #[get_size(ignore)]
  ignored_value: TestStructNoGetSize,
}

fn main() {
  let ignored_value = TestStructNoGetSize {
    value: "Hello world!".into(),
  };

  let test = TestStruct {
    name: "Adam".into(),
    ignored_value,
  };

  // Note that the result is lower then it should be.
  assert_eq!(test.get_heap_size(), 4);
}
```

### Returning a fixed value

In same cases you may be dealing with external types which allocate a fixed amount of bytes at the heap. In this case you may use the `size` attribute to always account the given field with a fixed value.

```rust
use get_size::GetSize;
#
# struct Buffer1024 {}
#
# impl Buffer1024 {
#   fn new() -> Self {
#      Self {}
#   }
# }

#[derive(GetSize)]
struct TestStruct {
  id: u64,
  #[get_size(size = 1024)]
  buffer: Buffer1024, // Always allocates exactly 1KB at the heap.
}

fn main() {
  let test = TestStruct {
    id: 1,
    buffer: Buffer1024::new(),
  };

  assert_eq!(test.get_heap_size(), 1024);
}
```

### Using a helper function

In same cases you may be dealing with an external data structure for which you know how to calculate its heap size using its public methods. In that case you may either use the newtype pattern to implement [`GetSize`] for it directly, or you can use the `size_fn` attribute, which will call the given function in order to calculate the fields heap size.

The latter is especially usefull if you can make use of a certain trait to calculate the heap size for multiple types.

Note that unlike in other crates, the name of the function to be called is **not** encapsulated by double-quotes ("), but rather given directly.

```rust
use get_size::GetSize;
#
# type ExternalVecAlike<T> = Vec<T>;

#[derive(GetSize)]
struct TestStruct {
  id: u64,
  #[get_size(size_fn = vec_alike_helper)]
  buffer: ExternalVecAlike<u8>,
}

// NOTE: We assume that slice.len()==slice.capacity()
fn vec_alike_helper<V, T>(slice: &V) -> usize
where
  V: AsRef<[T]>,
{
  std::mem::size_of::<T>() * slice.as_ref().len()
}

fn main() {
  let buffer = vec![0u8; 512];
  let buffer: ExternalVecAlike<u8> = buffer.into();

  let test = TestStruct {
    id: 1,
    buffer,
  };

  assert_eq!(test.get_heap_size(), 512);
}
```

### Ignoring certain generic types

If your struct uses generics, but the fields at which they are stored are ignored or get handled by helpers because the generic does not implement [`GetSize`], you will have to mark these generics with a special struct level `ignore` attribute. Otherwise the derived [`GetSize`] implementation would still require these generics to implement [`GetSize`], even through there is no need for it.

```rust
use get_size::GetSize;

#[derive(GetSize)]
#[get_size(ignore(B, C, D))]
struct TestStructHelpers<A, B, C, D> {
    value1: A,
    #[get_size(size = 100)]
    value2: B,
    #[get_size(size_fn = get_size_helper)]
    value3: C,
    #[get_size(ignore)]
    value4: D,
}

// Does not implement GetSize
struct NoGS {}

fn get_size_helper<C>(_value: &C) -> usize {
    50
}

fn main() {
    let test: TestStructHelpers<String, NoGS, NoGS, u64> = TestStructHelpers {
        value1: "Hello".into(),
        value2: NoGS {},
        value3: NoGS {},
        value4: 123,
    };

    assert_eq!(test.get_heap_size(), 5 + 100 + 50);
}
```
