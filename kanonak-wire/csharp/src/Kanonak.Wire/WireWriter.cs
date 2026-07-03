using System;
using System.Text;

namespace Kanonak.Wire
{
    /// <summary>
    /// An append-only buffer builder with validated writes.
    ///
    /// <para>The C# half of the Kanonak wire kernel's write side
    /// (<c>kanonak.org/wire-form</c>, <c>wireFormatVersion "1"</c>). Numeric
    /// parameters use exact-width types (<c>byte</c>/<c>ushort</c>/<c>uint</c>)
    /// — the type is the range validation. Strings are UTF-16, so
    /// <see cref="Utf8(string)"/> rejects unpaired surrogates with
    /// <c>InvalidUtf8</c> — never a lossy replacement character.</para>
    /// </summary>
    public sealed class WireWriter
    {
        private static readonly UTF8Encoding StrictUtf8 = new UTF8Encoding(
            encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

        private byte[] _buf;
        private int _len;

        public WireWriter()
            : this(64)
        {
        }

        public WireWriter(int capacity)
        {
            _buf = new byte[capacity < 1 ? 1 : capacity];
        }

        public static WireWriter WithCapacity(int capacity)
        {
            return new WireWriter(capacity);
        }

        private void Grow(int add)
        {
            if (_len + add <= _buf.Length)
            {
                return;
            }
            int cap = _buf.Length * 2;
            while (cap < _len + add)
            {
                cap *= 2;
            }
            var next = new byte[cap];
            Array.Copy(_buf, next, _len);
            _buf = next;
        }

        public WireWriter U8(byte value)
        {
            Grow(1);
            _buf[_len++] = value;
            return this;
        }

        public WireWriter U16Be(ushort value)
        {
            Grow(2);
            _buf[_len++] = (byte)(value >> 8);
            _buf[_len++] = (byte)value;
            return this;
        }

        public WireWriter U32Be(uint value)
        {
            Grow(4);
            _buf[_len++] = (byte)(value >> 24);
            _buf[_len++] = (byte)(value >> 16);
            _buf[_len++] = (byte)(value >> 8);
            _buf[_len++] = (byte)value;
            return this;
        }

        public WireWriter Bytes(ReadOnlySpan<byte> b)
        {
            Grow(b.Length);
            b.CopyTo(new Span<byte>(_buf, _len, b.Length));
            _len += b.Length;
            return this;
        }

        /// <summary>Hyphenated 8-4-4-4-12 hex, case-insensitive input; emits the 16 bytes.</summary>
        public WireWriter Uuid(string s)
        {
            if (!IsCanonicalUuidShape(s))
            {
                throw WireError.InvalidUuid($"\"{s}\" is not a hyphenated 8-4-4-4-12 UUID");
            }
            Grow(16);
            int i = 0;
            for (int oi = 0; oi < 16; oi++)
            {
                if (s[i] == '-')
                {
                    i++;
                }
                _buf[_len++] = (byte)((HexVal(s[i]) << 4) | HexVal(s[i + 1]));
                i += 2;
            }
            return this;
        }

        /// <summary>UTF-8 encode. Unpaired surrogates are InvalidUtf8 — never U+FFFD.</summary>
        public WireWriter Utf8(string s)
        {
            byte[] encoded;
            try
            {
                encoded = StrictUtf8.GetBytes(s);
            }
            catch (EncoderFallbackException)
            {
                throw WireError.InvalidUtf8(null, "string contains an unpaired surrogate");
            }
            return Bytes(encoded);
        }

        /// <summary>u16be length, then the bytes. Length above 0xFFFF is ValueOutOfRange.</summary>
        public WireWriter LenPrefixedBytes16(byte[] b)
        {
            if (b.Length > 0xffff)
            {
                throw WireError.ValueOutOfRange(b.Length, "lenPrefixedBytes16 length");
            }
            U16Be((ushort)b.Length);
            return Bytes(b);
        }

        /// <summary>The written bytes, exact length (a copy — the builder stays reusable).</summary>
        public byte[] ToBytes()
        {
            var result = new byte[_len];
            Array.Copy(_buf, result, _len);
            return result;
        }

        private static bool IsCanonicalUuidShape(string s)
        {
            if (s == null || s.Length != 36)
            {
                return false;
            }
            for (int i = 0; i < 36; i++)
            {
                bool hyphenPosition = i == 8 || i == 13 || i == 18 || i == 23;
                if (hyphenPosition)
                {
                    if (s[i] != '-')
                    {
                        return false;
                    }
                }
                else if (HexVal(s[i]) < 0)
                {
                    return false;
                }
            }
            return true;
        }

        private static int HexVal(char c)
        {
            if (c >= '0' && c <= '9')
            {
                return c - '0';
            }
            if (c >= 'a' && c <= 'f')
            {
                return c - 'a' + 10;
            }
            if (c >= 'A' && c <= 'F')
            {
                return c - 'A' + 10;
            }
            return -1;
        }
    }
}
