# kanonak-codec-jackson (Java)

The Jackson binding of the codec's `$`-envelope. `KanonakNode` binds no JSON key,
so that [`kanonak-codec`](../java) stays JDK-only. A generated class that Jackson
reads or writes takes its envelope from this module (runtime#56).

- Maven coordinates: **`org.kanonak:kanonak-codec-jackson`**, released with
  `kanonak-codec`, at its version
- Package: `org.kanonak.codec.jackson`
- Java 17; depends on `org.kanonak:kanonak-codec` and `jackson-databind`

## Use

```java
ObjectMapper json = JsonMapper.builder().addModule(new KanonakModule()).build();
// or .findAndAddModules(), which finds it by its META-INF/services entry
Order order = json.readValue(text, Order.class);   // Order extends KanonakNode
```

`KanonakModule` names each envelope accessor by its key: `$id`, `$type`, `$types`,
`$name`, `$contentHash` and `$version`. An envelope key that's absent is left out
on write, as `Codec.serialize(node, schema)` leaves it out.

What the module leaves to others:
- **The fields.** A generated class binds its own fields, and names on each
  integer or decimal field its datatype's serializer and deserializer, below.
- **`extra`.** The open-world assertions are left out of the binding, so the
  mapper's own configuration decides whether an unknown key is refused or
  ignored.

## The JSON forms of numbers (3.1.0)

`JsonForms` holds a serializer and a deserializer for each integer and decimal
datatype, so Jackson reads and writes a field as `Codec.serialize` and
`Codec.deserialize` do: a wide integer and a decimal as a string of their
digits, and `int` and narrower as a whole number.

```java
@JsonSerialize(using = JsonForms.XsdNonNegativeIntegerSerializer.class)
@JsonDeserialize(using = JsonForms.XsdNonNegativeIntegerDeserializer.class)
BigInteger cardinality;
```

| Datatype | Field type |
|---|---|
| `integer` and its four sign-restricted forms, `unsignedLong` | `BigInteger` |
| `long`, `unsignedInt` | `Long` |
| `decimal` | `BigDecimal` |
| `int`, `unsignedShort` | `Integer` |
| `short`, `unsignedByte` | `Short` |
| `byte` | `Byte` |

They're named on each field rather than registered for every `BigInteger`, so
adding a module never changes how a consumer's own fields are written. Each
refuses what the forms refuse, `"5"` for an `int` and `1.5` among them, which
Jackson would otherwise coerce, with a `JsonMappingException` ending
`[json-number-form]`. A JSON number is judged from its token.

## Conformance

Every node of the shared codec vectors, as given and as serialized, is read into
a class that binds no field of its own. It must give back each `$` key it
carries, then be written with exactly those keys and nothing of `extra`. The
command to run it is in `pom.xml`, and CI's `test-java` job runs it against the
Jackson jars the pom names, each pinned by digest. Each read and write case of
`codec-vectors-json-numbers.json` is run through `JsonForms` too, with each field
typed as a generated class types it.
