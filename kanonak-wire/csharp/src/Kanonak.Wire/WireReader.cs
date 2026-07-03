using System;
using System.Text;

namespace Kanonak.Wire
{
    /// <summary>
    /// A bounds-checked cursor over an immutable byte buffer. Never copies.
    ///
    /// <para>The C# half of the Kanonak wire kernel's read side
    /// (<c>kanonak.org/wire-form</c>, <c>wireFormatVersion "1"</c>).
    /// <see cref="Bytes(int)"/> and <see cref="Rest"/> return
    /// <see cref="ReadOnlyMemory{T}"/> views into the source buffer, never
    /// copies. Fail-loud contract: every failure is a <see cref="WireError"/>
    /// stating what was expected, what was found, and where.</para>
    /// </summary>
    public sealed class WireReader
    {
        private static readonly UTF8Encoding StrictUtf8 = new UTF8Encoding(
            encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

        private const string HexLower = "0123456789abcdef";

        private readonly byte[] _buf;
        private int _pos;

        public WireReader(byte[] buf)
        {
            if (buf == null)
            {
                throw new ArgumentNullException(nameof(buf));
            }
            _buf = buf;
        }

        /// <summary>Count of unread bytes.</summary>
        public int Remaining => _buf.Length - _pos;

        private void Need(int n, string context)
        {
            if (_buf.Length - _pos < n)
            {
                throw WireError.Truncated(n, _buf.Length - _pos, _pos, context);
            }
        }

        public byte U8()
        {
            Need(1, "u8");
            return _buf[_pos++];
        }

        public ushort U16Be()
        {
            Need(2, "u16be");
            ushort v = (ushort)((_buf[_pos] << 8) | _buf[_pos + 1]);
            _pos += 2;
            return v;
        }

        public uint U32Be()
        {
            Need(4, "u32be");
            int p = _pos;
            uint v = ((uint)_buf[p] << 24) | ((uint)_buf[p + 1] << 16) | ((uint)_buf[p + 2] << 8) | _buf[p + 3];
            _pos += 4;
            return v;
        }

        /// <summary>Exactly n bytes as a zero-copy view of the source buffer.</summary>
        public ReadOnlyMemory<byte> Bytes(int n)
        {
            Need(n, $"bytes({n})");
            var v = new ReadOnlyMemory<byte>(_buf, _pos, n);
            _pos += n;
            return v;
        }

        /// <summary>16 bytes as a lowercase hyphenated UUID string. Any 16 bytes are legal.</summary>
        public string Uuid()
        {
            Need(16, "uuid");
            var chars = new char[36];
            int ci = 0;
            for (int i = 0; i < 16; i++)
            {
                if (i == 4 || i == 6 || i == 8 || i == 10)
                {
                    chars[ci++] = '-';
                }
                byte b = _buf[_pos + i];
                chars[ci++] = HexLower[b >> 4];
                chars[ci++] = HexLower[b & 0x0f];
            }
            _pos += 16;
            return new string(chars);
        }

        /// <summary>n bytes decoded as STRICT UTF-8. Bounds are checked before validity.</summary>
        public string Utf8(int n)
        {
            int start = _pos;
            Bytes(n);
            try
            {
                return StrictUtf8.GetString(_buf, start, n);
            }
            catch (DecoderFallbackException)
            {
                _pos = start; // the read did not take effect
                throw WireError.InvalidUtf8(start, $"utf8({n})");
            }
        }

        /// <summary>u16be length L, then exactly L bytes (zero-copy view).</summary>
        public ReadOnlyMemory<byte> LenPrefixedBytes16()
        {
            int start = _pos;
            Need(2, "lenPrefixedBytes16");
            int declared = (_buf[_pos] << 8) | _buf[_pos + 1];
            int remainingAfterLength = _buf.Length - _pos - 2;
            if (declared > remainingAfterLength)
            {
                throw WireError.LengthOverrun(declared, remainingAfterLength, start, "lenPrefixedBytes16");
            }
            _pos += 2;
            var v = new ReadOnlyMemory<byte>(_buf, _pos, declared);
            _pos += declared;
            return v;
        }

        /// <summary>All remaining bytes (possibly empty) as a zero-copy view. Never errors.</summary>
        public ReadOnlyMemory<byte> Rest()
        {
            var v = new ReadOnlyMemory<byte>(_buf, _pos, _buf.Length - _pos);
            _pos = _buf.Length;
            return v;
        }

        /// <summary>Errors <c>TrailingBytes</c> if any bytes remain.</summary>
        public void ExpectEnd()
        {
            int count = _buf.Length - _pos;
            if (count > 0)
            {
                throw WireError.TrailingBytes(count, _pos);
            }
        }
    }
}
