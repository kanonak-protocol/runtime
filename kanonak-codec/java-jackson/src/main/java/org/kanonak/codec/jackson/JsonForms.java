package org.kanonak.codec.jackson;

import com.fasterxml.jackson.core.JsonGenerator;
import com.fasterxml.jackson.core.JsonParser;
import com.fasterxml.jackson.databind.DeserializationContext;
import com.fasterxml.jackson.databind.JsonMappingException;
import com.fasterxml.jackson.databind.SerializerProvider;
import com.fasterxml.jackson.databind.deser.std.StdDeserializer;
import com.fasterxml.jackson.databind.ser.std.StdSerializer;
import java.io.IOException;
import java.math.BigDecimal;
import java.math.BigInteger;
import org.kanonak.codec.Codec;
import org.kanonak.codec.JsonNumber;

/**
 * The Jackson binding of the JSON forms of integer and decimal values
 * (kanonak-protocol 4.11.0's {@code numbers-in-json}, runtime#54): a serializer
 * and a deserializer for each datatype, named on a generated class's field, so
 * Jackson reads and writes it as {@link Codec#serialize} and
 * {@link Codec#deserialize} do. For example:
 *
 * <pre>{@code
 * @JsonSerialize(using = JsonForms.XsdNonNegativeIntegerSerializer.class)
 * @JsonDeserialize(using = JsonForms.XsdNonNegativeIntegerDeserializer.class)
 * BigInteger cardinality;
 * }</pre>
 *
 * <p>Named on each field rather than registered for every {@link BigInteger},
 * so registering a module never changes how a consumer's own fields are
 * written. Each is parameterized by its datatype, since Jackson coerces
 * {@code "5"} into an {@code int} and truncates {@code 1.5}, which the forms
 * refuse. A JSON number is passed to the codec as its token, so it is judged
 * exactly. Each refusal is a {@link JsonMappingException} whose message ends
 * {@code [json-number-form]}.
 */
public final class JsonForms {
    private JsonForms() {
    }

    /** Reads one datatype's values from their JSON form. */
    public abstract static class FormDeserializer<T> extends StdDeserializer<T> {
        private static final long serialVersionUID = 1L;
        private final String datatype;

        protected FormDeserializer(Class<T> type, String datatype) {
            super(type);
            this.datatype = datatype;
        }

        /** The exact value {@link Codec#readJsonForm} gives, as this deserializer's type. */
        protected abstract T fromExact(Object exact);

        @Override
        public T deserialize(JsonParser p, DeserializationContext ctxt) throws IOException {
            Object json = switch (p.currentToken()) {
                case VALUE_STRING -> p.getText();
                case VALUE_NUMBER_INT, VALUE_NUMBER_FLOAT -> new JsonNumber(p.getText());
                case VALUE_TRUE -> Boolean.TRUE;
                case VALUE_FALSE -> Boolean.FALSE;
                default -> throw JsonMappingException.from(p,
                    "xsd:" + datatype + ": a JSON " + p.currentToken() + " isn't a value of it [json-number-form]");
            };
            try {
                return fromExact(Codec.readJsonForm(datatype, json));
            } catch (IllegalArgumentException refused) {
                throw JsonMappingException.from(p, refused.getMessage(), refused);
            }
        }
    }

    /** Writes one datatype's values in their JSON form. */
    public abstract static class FormSerializer<T> extends StdSerializer<T> {
        private static final long serialVersionUID = 1L;
        private final String datatype;

        protected FormSerializer(Class<T> type, String datatype) {
            super(type);
            this.datatype = datatype;
        }

        @Override
        public void serialize(T value, JsonGenerator g, SerializerProvider provider) throws IOException {
            Object form;
            try {
                form = Codec.writeJsonForm(datatype, value);
            } catch (IllegalArgumentException refused) {
                throw JsonMappingException.from(g, refused.getMessage(), refused);
            }
            if (form instanceof String digits) {
                g.writeString(digits);
            } else {
                g.writeNumber((Long) form);
            }
        }
    }

    /** Reads {@code xsd:integer} as a {@link BigInteger}. */
    public static final class XsdIntegerDeserializer extends FormDeserializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdIntegerDeserializer() {
            super(BigInteger.class, "integer");
        }

        @Override
        protected BigInteger fromExact(Object exact) {
            return (BigInteger) exact;
        }
    }

    /** Writes {@code xsd:integer} from a {@link BigInteger}. */
    public static final class XsdIntegerSerializer extends FormSerializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdIntegerSerializer() {
            super(BigInteger.class, "integer");
        }
    }

    /** Reads {@code xsd:nonNegativeInteger} as a {@link BigInteger}. */
    public static final class XsdNonNegativeIntegerDeserializer extends FormDeserializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdNonNegativeIntegerDeserializer() {
            super(BigInteger.class, "nonNegativeInteger");
        }

        @Override
        protected BigInteger fromExact(Object exact) {
            return (BigInteger) exact;
        }
    }

    /** Writes {@code xsd:nonNegativeInteger} from a {@link BigInteger}. */
    public static final class XsdNonNegativeIntegerSerializer extends FormSerializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdNonNegativeIntegerSerializer() {
            super(BigInteger.class, "nonNegativeInteger");
        }
    }

    /** Reads {@code xsd:positiveInteger} as a {@link BigInteger}. */
    public static final class XsdPositiveIntegerDeserializer extends FormDeserializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdPositiveIntegerDeserializer() {
            super(BigInteger.class, "positiveInteger");
        }

        @Override
        protected BigInteger fromExact(Object exact) {
            return (BigInteger) exact;
        }
    }

    /** Writes {@code xsd:positiveInteger} from a {@link BigInteger}. */
    public static final class XsdPositiveIntegerSerializer extends FormSerializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdPositiveIntegerSerializer() {
            super(BigInteger.class, "positiveInteger");
        }
    }

    /** Reads {@code xsd:nonPositiveInteger} as a {@link BigInteger}. */
    public static final class XsdNonPositiveIntegerDeserializer extends FormDeserializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdNonPositiveIntegerDeserializer() {
            super(BigInteger.class, "nonPositiveInteger");
        }

        @Override
        protected BigInteger fromExact(Object exact) {
            return (BigInteger) exact;
        }
    }

    /** Writes {@code xsd:nonPositiveInteger} from a {@link BigInteger}. */
    public static final class XsdNonPositiveIntegerSerializer extends FormSerializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdNonPositiveIntegerSerializer() {
            super(BigInteger.class, "nonPositiveInteger");
        }
    }

    /** Reads {@code xsd:negativeInteger} as a {@link BigInteger}. */
    public static final class XsdNegativeIntegerDeserializer extends FormDeserializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdNegativeIntegerDeserializer() {
            super(BigInteger.class, "negativeInteger");
        }

        @Override
        protected BigInteger fromExact(Object exact) {
            return (BigInteger) exact;
        }
    }

    /** Writes {@code xsd:negativeInteger} from a {@link BigInteger}. */
    public static final class XsdNegativeIntegerSerializer extends FormSerializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdNegativeIntegerSerializer() {
            super(BigInteger.class, "negativeInteger");
        }
    }

    /** Reads {@code xsd:long} as a {@link Long}. */
    public static final class XsdLongDeserializer extends FormDeserializer<Long> {
        private static final long serialVersionUID = 1L;

        public XsdLongDeserializer() {
            super(Long.class, "long");
        }

        @Override
        protected Long fromExact(Object exact) {
            return (Long) exact;
        }
    }

    /** Writes {@code xsd:long} from a {@link Long}. */
    public static final class XsdLongSerializer extends FormSerializer<Long> {
        private static final long serialVersionUID = 1L;

        public XsdLongSerializer() {
            super(Long.class, "long");
        }
    }

    /** Reads {@code xsd:unsignedLong} as a {@link BigInteger}. */
    public static final class XsdUnsignedLongDeserializer extends FormDeserializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdUnsignedLongDeserializer() {
            super(BigInteger.class, "unsignedLong");
        }

        @Override
        protected BigInteger fromExact(Object exact) {
            return (BigInteger) exact;
        }
    }

    /** Writes {@code xsd:unsignedLong} from a {@link BigInteger}. */
    public static final class XsdUnsignedLongSerializer extends FormSerializer<BigInteger> {
        private static final long serialVersionUID = 1L;

        public XsdUnsignedLongSerializer() {
            super(BigInteger.class, "unsignedLong");
        }
    }

    /** Reads {@code xsd:decimal} as a {@link BigDecimal}. */
    public static final class XsdDecimalDeserializer extends FormDeserializer<BigDecimal> {
        private static final long serialVersionUID = 1L;

        public XsdDecimalDeserializer() {
            super(BigDecimal.class, "decimal");
        }

        @Override
        protected BigDecimal fromExact(Object exact) {
            return (BigDecimal) exact;
        }
    }

    /** Writes {@code xsd:decimal} from a {@link BigDecimal}. */
    public static final class XsdDecimalSerializer extends FormSerializer<BigDecimal> {
        private static final long serialVersionUID = 1L;

        public XsdDecimalSerializer() {
            super(BigDecimal.class, "decimal");
        }
    }

    /** Reads {@code xsd:int} as a {@link Integer}. */
    public static final class XsdIntDeserializer extends FormDeserializer<Integer> {
        private static final long serialVersionUID = 1L;

        public XsdIntDeserializer() {
            super(Integer.class, "int");
        }

        @Override
        protected Integer fromExact(Object exact) {
            return Math.toIntExact((Long) exact);
        }
    }

    /** Writes {@code xsd:int} from a {@link Integer}. */
    public static final class XsdIntSerializer extends FormSerializer<Integer> {
        private static final long serialVersionUID = 1L;

        public XsdIntSerializer() {
            super(Integer.class, "int");
        }
    }

    /** Reads {@code xsd:short} as a {@link Short}. */
    public static final class XsdShortDeserializer extends FormDeserializer<Short> {
        private static final long serialVersionUID = 1L;

        public XsdShortDeserializer() {
            super(Short.class, "short");
        }

        @Override
        protected Short fromExact(Object exact) {
            return (short) (long) (Long) exact;
        }
    }

    /** Writes {@code xsd:short} from a {@link Short}. */
    public static final class XsdShortSerializer extends FormSerializer<Short> {
        private static final long serialVersionUID = 1L;

        public XsdShortSerializer() {
            super(Short.class, "short");
        }
    }

    /** Reads {@code xsd:byte} as a {@link Byte}. */
    public static final class XsdByteDeserializer extends FormDeserializer<Byte> {
        private static final long serialVersionUID = 1L;

        public XsdByteDeserializer() {
            super(Byte.class, "byte");
        }

        @Override
        protected Byte fromExact(Object exact) {
            return (byte) (long) (Long) exact;
        }
    }

    /** Writes {@code xsd:byte} from a {@link Byte}. */
    public static final class XsdByteSerializer extends FormSerializer<Byte> {
        private static final long serialVersionUID = 1L;

        public XsdByteSerializer() {
            super(Byte.class, "byte");
        }
    }

    /** Reads {@code xsd:unsignedInt} as a {@link Long}. */
    public static final class XsdUnsignedIntDeserializer extends FormDeserializer<Long> {
        private static final long serialVersionUID = 1L;

        public XsdUnsignedIntDeserializer() {
            super(Long.class, "unsignedInt");
        }

        @Override
        protected Long fromExact(Object exact) {
            return (Long) exact;
        }
    }

    /** Writes {@code xsd:unsignedInt} from a {@link Long}. */
    public static final class XsdUnsignedIntSerializer extends FormSerializer<Long> {
        private static final long serialVersionUID = 1L;

        public XsdUnsignedIntSerializer() {
            super(Long.class, "unsignedInt");
        }
    }

    /** Reads {@code xsd:unsignedShort} as a {@link Integer}. */
    public static final class XsdUnsignedShortDeserializer extends FormDeserializer<Integer> {
        private static final long serialVersionUID = 1L;

        public XsdUnsignedShortDeserializer() {
            super(Integer.class, "unsignedShort");
        }

        @Override
        protected Integer fromExact(Object exact) {
            return Math.toIntExact((Long) exact);
        }
    }

    /** Writes {@code xsd:unsignedShort} from a {@link Integer}. */
    public static final class XsdUnsignedShortSerializer extends FormSerializer<Integer> {
        private static final long serialVersionUID = 1L;

        public XsdUnsignedShortSerializer() {
            super(Integer.class, "unsignedShort");
        }
    }

    /** Reads {@code xsd:unsignedByte} as a {@link Short}. */
    public static final class XsdUnsignedByteDeserializer extends FormDeserializer<Short> {
        private static final long serialVersionUID = 1L;

        public XsdUnsignedByteDeserializer() {
            super(Short.class, "unsignedByte");
        }

        @Override
        protected Short fromExact(Object exact) {
            return (short) (long) (Long) exact;
        }
    }

    /** Writes {@code xsd:unsignedByte} from a {@link Short}. */
    public static final class XsdUnsignedByteSerializer extends FormSerializer<Short> {
        private static final long serialVersionUID = 1L;

        public XsdUnsignedByteSerializer() {
            super(Short.class, "unsignedByte");
        }
    }
}
