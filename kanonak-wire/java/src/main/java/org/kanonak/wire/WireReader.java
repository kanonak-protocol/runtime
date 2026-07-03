package org.kanonak.wire;

import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CharsetDecoder;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;

/**
 * A bounds-checked cursor over an immutable byte buffer. Never copies.
 *
 * <p>The Java port of the Kanonak wire kernel reader ({@code kanonak.org/wire-form},
 * {@code wireFormatVersion "1"}). {@link #bytes(int)} and {@link #rest()} return
 * read-only {@link ByteBuffer} VIEWS over the backing array — zero-copy; copying
 * is always an explicit caller decision. {@code u32be} widens to {@code long}
 * because Java has no unsigned 32-bit type. UTF-8 decoding is STRICT
 * ({@link CodingErrorAction#REPORT}) — invalid sequences, overlong encodings,
 * and surrogate-range encodings are {@code InvalidUtf8}, never U+FFFD.
 */
public final class WireReader {
    private static final char[] HEX_LOWER = "0123456789abcdef".toCharArray();

    private final byte[] buf;
    private int pos = 0;

    public WireReader(byte[] buf) {
        this.buf = buf;
    }

    private void need(int n, String context) {
        int remaining = buf.length - pos;
        if (remaining < n) {
            throw WireError.truncated(n, remaining, pos, context);
        }
    }

    /** One byte, 0..255. */
    public int u8() {
        need(1, "u8");
        return buf[pos++] & 0xFF;
    }

    /** Two bytes, big-endian, 0..65535. */
    public int u16be() {
        need(2, "u16be");
        int v = ((buf[pos] & 0xFF) << 8) | (buf[pos + 1] & 0xFF);
        pos += 2;
        return v;
    }

    /** Four bytes, big-endian, 0..2^32-1 — widened to {@code long} (Java has no u32). */
    public long u32be() {
        need(4, "u32be");
        long v = (((long) (buf[pos] & 0xFF) << 24)
            | ((buf[pos + 1] & 0xFF) << 16)
            | ((buf[pos + 2] & 0xFF) << 8)
            | (buf[pos + 3] & 0xFF)) & 0xFFFFFFFFL;
        pos += 4;
        return v;
    }

    /** Exactly n bytes as a zero-copy read-only {@link ByteBuffer} view. */
    public ByteBuffer bytes(int n) {
        need(n, "bytes(" + n + ")");
        ByteBuffer v = ByteBuffer.wrap(buf, pos, n).slice().asReadOnlyBuffer();
        pos += n;
        return v;
    }

    /** 16 bytes as a lowercase hyphenated UUID string. Any 16 bytes are legal. */
    public String uuid() {
        need(16, "uuid");
        StringBuilder s = new StringBuilder(36);
        for (int i = 0; i < 16; i++) {
            if (i == 4 || i == 6 || i == 8 || i == 10) {
                s.append('-');
            }
            int b = buf[pos + i] & 0xFF;
            s.append(HEX_LOWER[b >>> 4]).append(HEX_LOWER[b & 0x0F]);
        }
        pos += 16;
        return s.toString();
    }

    /**
     * n bytes decoded as STRICT UTF-8. Bounds are checked before validity
     * ({@code Truncated} beats {@code InvalidUtf8}); the position does NOT
     * advance on a decode failure.
     */
    public String utf8(int n) {
        int start = pos;
        need(n, "bytes(" + n + ")");
        CharsetDecoder decoder = StandardCharsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT);
        try {
            String s = decoder.decode(ByteBuffer.wrap(buf, start, n)).toString();
            pos = start + n;
            return s;
        } catch (CharacterCodingException e) {
            // the read did not take effect — pos stays at start
            throw WireError.invalidUtf8(start, "utf8(" + n + ")");
        }
    }

    /** u16be length L, then exactly L bytes (zero-copy view). L beyond remaining is LengthOverrun. */
    public ByteBuffer lenPrefixedBytes16() {
        int start = pos;
        need(2, "lenPrefixedBytes16");
        int declared = ((buf[pos] & 0xFF) << 8) | (buf[pos + 1] & 0xFF);
        int remainingAfterLength = buf.length - pos - 2;
        if (declared > remainingAfterLength) {
            throw WireError.lengthOverrun(declared, remainingAfterLength, start, "lenPrefixedBytes16");
        }
        pos += 2;
        ByteBuffer v = ByteBuffer.wrap(buf, pos, declared).slice().asReadOnlyBuffer();
        pos += declared;
        return v;
    }

    /** All remaining bytes (possibly empty) as a zero-copy view. Never errors. */
    public ByteBuffer rest() {
        ByteBuffer v = ByteBuffer.wrap(buf, pos, buf.length - pos).slice().asReadOnlyBuffer();
        pos = buf.length;
        return v;
    }

    /** Count of unread bytes. */
    public int remaining() {
        return buf.length - pos;
    }

    /** Errors {@code TrailingBytes} if any bytes remain. */
    public void expectEnd() {
        int count = buf.length - pos;
        if (count > 0) {
            throw WireError.trailingBytes(count, pos);
        }
    }
}
