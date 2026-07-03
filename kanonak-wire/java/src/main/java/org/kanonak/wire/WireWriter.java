package org.kanonak.wire;

import java.nio.ByteBuffer;
import java.nio.CharBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CharsetEncoder;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/**
 * An append-only buffer builder with validated writes.
 *
 * <p>The Java port of the Kanonak wire kernel writer ({@code kanonak.org/wire-form},
 * {@code wireFormatVersion "1"}). Java's {@code int}/{@code long} parameters can
 * exceed the wire type's range, so every numeric write validates: out-of-range
 * values are {@code ValueOutOfRange} — never a silent truncation. UTF-8 encoding
 * is STRICT ({@link CodingErrorAction#REPORT}) — a Java {@link String} can hold
 * an unpaired surrogate, which is {@code InvalidUtf8}, never U+FFFD.
 */
public final class WireWriter {
    private byte[] buf;
    private int len = 0;

    public WireWriter() {
        this(64);
    }

    public WireWriter(int capacity) {
        this.buf = new byte[capacity < 1 ? 1 : capacity];
    }

    public static WireWriter withCapacity(int capacity) {
        return new WireWriter(capacity);
    }

    private void grow(int add) {
        if (len + add <= buf.length) {
            return;
        }
        int cap = buf.length * 2;
        while (cap < len + add) {
            cap *= 2;
        }
        buf = Arrays.copyOf(buf, cap);
    }

    private void uint(long value, long max, String type) {
        if (value < 0 || value > max) {
            throw WireError.valueOutOfRange(value, type);
        }
    }

    /** One byte. Values outside 0..255 are ValueOutOfRange. */
    public WireWriter u8(int value) {
        uint(value, 0xFF, "u8");
        grow(1);
        buf[len++] = (byte) value;
        return this;
    }

    /** Two bytes, big-endian. Values outside 0..65535 are ValueOutOfRange. */
    public WireWriter u16be(int value) {
        uint(value, 0xFFFF, "u16be");
        grow(2);
        buf[len++] = (byte) (value >>> 8);
        buf[len++] = (byte) value;
        return this;
    }

    /** Four bytes, big-endian. Values outside 0..2^32-1 are ValueOutOfRange. */
    public WireWriter u32be(long value) {
        uint(value, 0xFFFFFFFFL, "u32be");
        grow(4);
        buf[len++] = (byte) (value >>> 24);
        buf[len++] = (byte) (value >>> 16);
        buf[len++] = (byte) (value >>> 8);
        buf[len++] = (byte) value;
        return this;
    }

    /** Appends the bytes verbatim. */
    public WireWriter bytes(byte[] b) {
        grow(b.length);
        System.arraycopy(b, 0, buf, len, b.length);
        len += b.length;
        return this;
    }

    /** Appends the buffer's remaining bytes verbatim (the caller's position is untouched). */
    public WireWriter bytes(ByteBuffer b) {
        ByteBuffer view = b.duplicate();
        int n = view.remaining();
        grow(n);
        view.get(buf, len, n);
        len += n;
        return this;
    }

    /** Hyphenated 8-4-4-4-12 hex, case-insensitive input; emits the 16 bytes. */
    public WireWriter uuid(String s) {
        if (s.length() != 36
            || s.charAt(8) != '-' || s.charAt(13) != '-'
            || s.charAt(18) != '-' || s.charAt(23) != '-') {
            throw WireError.invalidUuid("\"" + s + "\" is not a hyphenated 8-4-4-4-12 UUID");
        }
        byte[] out = new byte[16];
        int oi = 0;
        int i = 0;
        while (i < 36) {
            if (s.charAt(i) == '-') {
                i++;
                continue;
            }
            int hi = hexVal(s.charAt(i));
            int lo = i + 1 < 36 ? hexVal(s.charAt(i + 1)) : -1;
            if (hi < 0 || lo < 0) {
                throw WireError.invalidUuid("\"" + s + "\" is not a hyphenated 8-4-4-4-12 UUID");
            }
            out[oi++] = (byte) ((hi << 4) | lo);
            i += 2;
        }
        return bytes(out);
    }

    /** UTF-8 encode. Unpaired surrogates are InvalidUtf8 — never U+FFFD or '?'. */
    public WireWriter utf8(String s) {
        CharsetEncoder encoder = StandardCharsets.UTF_8.newEncoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT);
        ByteBuffer encoded;
        try {
            encoded = encoder.encode(CharBuffer.wrap(s));
        } catch (CharacterCodingException e) {
            throw WireError.invalidUtf8(null, "string contains an unpaired surrogate");
        }
        return bytes(encoded);
    }

    /** u16be length, then the bytes. Length above 0xFFFF is ValueOutOfRange. */
    public WireWriter lenPrefixedBytes16(byte[] b) {
        if (b.length > 0xFFFF) {
            throw WireError.valueOutOfRange(b.length, "lenPrefixedBytes16 length");
        }
        u16be(b.length);
        return bytes(b);
    }

    /** The written bytes, exact length (a copy — the builder stays reusable). */
    public byte[] toBytes() {
        return Arrays.copyOf(buf, len);
    }

    private static int hexVal(char c) {
        if (c >= '0' && c <= '9') {
            return c - '0';
        }
        if (c >= 'a' && c <= 'f') {
            return c - 'a' + 10;
        }
        if (c >= 'A' && c <= 'F') {
            return c - 'A' + 10;
        }
        return -1;
    }
}
