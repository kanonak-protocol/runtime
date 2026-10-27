# kanonak-codec

The generic Kanonak codec runtime (Python): builds the canonical input model from typed nodes +
an embedded schema, content-addresses it via `kanonak-canonical`, and (de)serializes the
normalized-JSON wire form. Referenced by generated typed SDKs.

`serialize_exact(node, schema)` and `deserialize_exact(json_obj, schema)` (2.2.0) carry each integer
and decimal value in its JSON form (kanonak-protocol 4.11.0's `numbers-in-json`, runtime#54), through
lists and embedded values: `deserialize_exact` gives every integer as an `int` and a `decimal` as a
`decimal.Decimal`. A JSON number past ±(2^53 − 1) for a wide integer, a number for a decimal, a string for a narrow integer, or a value outside its datatype's range is refused, ending `[json-number-form]`, as `codec-vectors-json-numbers.json` pins in every port. `serialize` and `deserialize` are deprecated for them.

Source & issues: https://github.com/kanonak-protocol/runtime
