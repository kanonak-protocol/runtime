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
on write, as `Codec.serialize` leaves it out.

What the module leaves to others:
- **The fields.** A generated class binds its own fields.
- **`extra`.** The open-world assertions are left out of the binding, so the
  mapper's own configuration decides whether an unknown key is refused or
  ignored.

## Conformance

Every node of the shared codec vectors, as given and as serialized, is read into
a class that binds no field of its own. It must give back each `$` key it
carries, then be written with exactly those keys and nothing of `extra`. The
command to run it is in `pom.xml`, and CI's `test-java` job runs it against the
Jackson jars the pom names, each pinned by digest.
