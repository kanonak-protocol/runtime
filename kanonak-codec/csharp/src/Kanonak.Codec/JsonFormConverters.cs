// The System.Text.Json binding of the JSON forms of integer and decimal values (kanonak-protocol 4.11.0's
// numbers-in-json, runtime#54): a converter for each datatype, named on a generated class's property with
// [JsonConverter(typeof(...))], so System.Text.Json reads and writes it as Codec.Serialize and Codec.Deserialize do.
using System;
using System.Globalization;
using System.Numerics;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Kanonak.Codec
{
    /// <summary>
    /// A converter of one xsd datatype's values to and from their JSON form. Each refusal is a
    /// <see cref="JsonException"/> whose message ends <c>[json-number-form]</c>.
    /// </summary>
    public abstract class JsonFormConverter<T> : JsonConverter<T>
    {
        private readonly string _localName;

        /// <param name="localName">The datatype's local name in core-xsd.</param>
        protected JsonFormConverter(string localName)
        {
            _localName = localName;
        }

        /// <summary>The exact value <see cref="Codec"/> reads, as this converter's type.</summary>
        protected abstract T FromExact(object exact);

        public override T Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
        {
            object json;
            switch (reader.TokenType)
            {
                case JsonTokenType.String: json = reader.GetString(); break;
                case JsonTokenType.Number: json = NumberToken(TokenText(ref reader)); break;
                case JsonTokenType.True: json = true; break;
                case JsonTokenType.False: json = false; break;
                default: throw new JsonException("xsd:" + _localName + ": a JSON " + reader.TokenType + " isn't a value of it [json-number-form]");
            }
            try
            {
                return FromExact(Codec.ReadJsonNumberOf(_localName, json));
            }
            catch (ArgumentException e)
            {
                throw new JsonException(e.Message, e);
            }
        }

        public override void Write(Utf8JsonWriter writer, T value, JsonSerializerOptions options)
        {
            object form;
            try
            {
                form = Codec.WriteJsonNumberOf(_localName, value);
            }
            catch (ArgumentException e)
            {
                throw new JsonException(e.Message, e);
            }
            if (form is string digits) writer.WriteStringValue(digits);
            else writer.WriteNumberValue(Convert.ToInt64(form, CultureInfo.InvariantCulture));
        }

        private static string TokenText(ref Utf8JsonReader reader)
        {
            byte[] bytes = reader.HasValueSequence ? ToArray(reader.ValueSequence) : reader.ValueSpan.ToArray();
            return Encoding.UTF8.GetString(bytes);
        }

        private static byte[] ToArray(System.Buffers.ReadOnlySequence<byte> sequence)
        {
            var bytes = new byte[sequence.Length];
            int at = 0;
            foreach (var segment in sequence)
            {
                segment.Span.CopyTo(bytes.AsSpan(at));
                at += segment.Length;
            }
            return bytes;
        }

        /// <summary>
        /// A JSON number token, judged exactly from its digits: a whole number as a <see cref="long"/> when it
        /// fits, and otherwise the <see cref="double"/> JSON's readers would take it for, which the forms refuse.
        /// A <see cref="decimal"/> would round past 28 digits, taking 1.00000000000000000000000000001 for 1.
        /// </summary>
        private static object NumberToken(string text)
        {
            double asDouble = double.Parse(text, NumberStyles.Float, CultureInfo.InvariantCulture);
            int e = text.IndexOfAny(new[] { 'e', 'E' });
            string mantissa = e < 0 ? text : text.Substring(0, e);
            int exponent = 0;
            if (e >= 0 && !int.TryParse(text.Substring(e + 1), NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out exponent))
                return asDouble;
            bool negative = mantissa.StartsWith("-", StringComparison.Ordinal);
            if (negative) mantissa = mantissa.Substring(1);
            int dot = mantissa.IndexOf('.');
            string digits = dot < 0 ? mantissa : mantissa.Remove(dot, 1);
            int scale = (dot < 0 ? 0 : mantissa.Length - dot - 1) - exponent;
            if (Math.Abs(scale) > 400) return asDouble;
            BigInteger n = BigInteger.Parse(digits, CultureInfo.InvariantCulture);
            if (scale > 0)
            {
                BigInteger unit = BigInteger.Pow(10, scale);
                if (!(n % unit).IsZero) return asDouble;
                n /= unit;
            }
            else if (scale < 0)
            {
                n *= BigInteger.Pow(10, -scale);
            }
            if (negative) n = -n;
            return n >= long.MinValue && n <= long.MaxValue ? (object)(long)n : asDouble;
        }
    }

    /// <summary><c>xsd:integer</c> as a <see cref="BigInteger"/>, which JSON carries as a string of its digits.</summary>
    public sealed class IntegerJsonConverter : JsonFormConverter<BigInteger>
    {
        public IntegerJsonConverter() : base("integer") { }
        protected override BigInteger FromExact(object exact) => (BigInteger)exact;
    }

    /// <summary><c>xsd:nonNegativeInteger</c> as a <see cref="BigInteger"/>, as a string of its digits.</summary>
    public sealed class NonNegativeIntegerJsonConverter : JsonFormConverter<BigInteger>
    {
        public NonNegativeIntegerJsonConverter() : base("nonNegativeInteger") { }
        protected override BigInteger FromExact(object exact) => (BigInteger)exact;
    }

    /// <summary><c>xsd:positiveInteger</c> as a <see cref="BigInteger"/>, as a string of its digits.</summary>
    public sealed class PositiveIntegerJsonConverter : JsonFormConverter<BigInteger>
    {
        public PositiveIntegerJsonConverter() : base("positiveInteger") { }
        protected override BigInteger FromExact(object exact) => (BigInteger)exact;
    }

    /// <summary><c>xsd:nonPositiveInteger</c> as a <see cref="BigInteger"/>, as a string of its digits.</summary>
    public sealed class NonPositiveIntegerJsonConverter : JsonFormConverter<BigInteger>
    {
        public NonPositiveIntegerJsonConverter() : base("nonPositiveInteger") { }
        protected override BigInteger FromExact(object exact) => (BigInteger)exact;
    }

    /// <summary><c>xsd:negativeInteger</c> as a <see cref="BigInteger"/>, as a string of its digits.</summary>
    public sealed class NegativeIntegerJsonConverter : JsonFormConverter<BigInteger>
    {
        public NegativeIntegerJsonConverter() : base("negativeInteger") { }
        protected override BigInteger FromExact(object exact) => (BigInteger)exact;
    }

    /// <summary><c>xsd:long</c> as a <see cref="long"/>, which JSON carries as a string of its digits.</summary>
    public sealed class LongJsonConverter : JsonFormConverter<long>
    {
        public LongJsonConverter() : base("long") { }
        protected override long FromExact(object exact) => (long)exact;
    }

    /// <summary><c>xsd:unsignedLong</c> as a <see cref="ulong"/>, which JSON carries as a string of its digits.</summary>
    public sealed class UnsignedLongJsonConverter : JsonFormConverter<ulong>
    {
        public UnsignedLongJsonConverter() : base("unsignedLong") { }
        protected override ulong FromExact(object exact) => (ulong)exact;
    }

    /// <summary>
    /// <c>xsd:decimal</c> as a string of its canonical digits, as <see cref="Codec.Deserialize"/> gives it, since
    /// System.Decimal holds only 28 to 29 digits. JSON carries it as that string, and it reads from a string only.
    /// </summary>
    public sealed class DecimalJsonConverter : JsonFormConverter<string>
    {
        public DecimalJsonConverter() : base("decimal") { }
        protected override string FromExact(object exact) => (string)exact;
    }

    /// <summary><c>xsd:int</c> as an <see cref="int"/>, read from and written as a whole JSON number only.</summary>
    public sealed class IntJsonConverter : JsonFormConverter<int>
    {
        public IntJsonConverter() : base("int") { }
        protected override int FromExact(object exact) => Convert.ToInt32(exact, CultureInfo.InvariantCulture);
    }

    /// <summary><c>xsd:short</c> as a <see cref="short"/>, as a whole JSON number only.</summary>
    public sealed class ShortJsonConverter : JsonFormConverter<short>
    {
        public ShortJsonConverter() : base("short") { }
        protected override short FromExact(object exact) => Convert.ToInt16(exact, CultureInfo.InvariantCulture);
    }

    /// <summary><c>xsd:byte</c> as an <see cref="sbyte"/>, as a whole JSON number only.</summary>
    public sealed class ByteJsonConverter : JsonFormConverter<sbyte>
    {
        public ByteJsonConverter() : base("byte") { }
        protected override sbyte FromExact(object exact) => Convert.ToSByte(exact, CultureInfo.InvariantCulture);
    }

    /// <summary><c>xsd:unsignedInt</c> as a <see cref="uint"/>, as a whole JSON number only.</summary>
    public sealed class UnsignedIntJsonConverter : JsonFormConverter<uint>
    {
        public UnsignedIntJsonConverter() : base("unsignedInt") { }
        protected override uint FromExact(object exact) => Convert.ToUInt32(exact, CultureInfo.InvariantCulture);
    }

    /// <summary><c>xsd:unsignedShort</c> as a <see cref="ushort"/>, as a whole JSON number only.</summary>
    public sealed class UnsignedShortJsonConverter : JsonFormConverter<ushort>
    {
        public UnsignedShortJsonConverter() : base("unsignedShort") { }
        protected override ushort FromExact(object exact) => Convert.ToUInt16(exact, CultureInfo.InvariantCulture);
    }

    /// <summary><c>xsd:unsignedByte</c> as a <see cref="byte"/>, as a whole JSON number only.</summary>
    public sealed class UnsignedByteJsonConverter : JsonFormConverter<byte>
    {
        public UnsignedByteJsonConverter() : base("unsignedByte") { }
        protected override byte FromExact(object exact) => Convert.ToByte(exact, CultureInfo.InvariantCulture);
    }
}
