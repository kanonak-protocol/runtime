# kanonak-codec

The generic Kanonak codec runtime (Python): builds the canonical input model from typed nodes +
an embedded schema, content-addresses it via `kanonak-canonical`, and (de)serializes the
normalized-JSON wire form. Referenced by generated typed SDKs.

`serialize(node, schema)` and `deserialize(json_obj, schema)` are the normalized-JSON wire form. From 3.0.0
they carry each integer and decimal value in its JSON form (kanonak-protocol 4.11.0's `numbers-in-json`,
runtime#54), through lists and embedded values: `deserialize` gives every integer as an `int` and a `decimal` as a
`decimal.Decimal`, and `serialize` writes a wide integer and a decimal as a string of their canonical digits, and a
narrow integer as an `int`. An embedded value whose class the schema doesn't hold is carried as given. `deserialize`
judges the values it's given: `json.loads` reads `1.0000000000000000001` as the float `1.0`,
so a caller that must refuse it parses with `json.loads(text, parse_float=decimal.Decimal)`, whose numbers it judges
exactly. A JSON number past ±(2^53 − 1) for a wide integer, a number for a decimal, a string for a narrow integer, or a value outside its datatype's range is refused, ending `[json-number-form]`, as `codec-vectors-json-numbers.json` pins in every port.

From 3.1.0, `kanonak-codec[pydantic]` gives a generated model's fields the pydantic binding of the same forms, in
`kanonak_codec.pydantic`: `Integer`, `NonNegativeInteger`, `PositiveInteger`, `NonPositiveInteger`,
`NegativeInteger`, `Long`, `UnsignedLong` and `Decimal`, and `Int`, `Short`, `Byte`, `UnsignedInt`, `UnsignedShort`
and `UnsignedByte`, since pydantic's own `int` reads `"5"`. Each is an annotated type named for its datatype, which
reads JSON as `deserialize` does, writes it as `serialize` does, and refuses what the forms refuse, ending
`[json-number-form]`. A value given from Python is taken when it holds the datatype's value exactly. The same vectors
gate it, through pydantic.

Source & issues: https://github.com/kanonak-protocol/runtime
