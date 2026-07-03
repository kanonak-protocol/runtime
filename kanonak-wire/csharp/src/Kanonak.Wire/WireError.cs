using System;

namespace Kanonak.Wire
{
    /// <summary>
    /// A wire kernel error: what was expected, what was found, and where.
    ///
    /// <para>The error taxonomy of <c>wireFormatVersion "1"</c> — <c>Truncated</c>,
    /// <c>LengthOverrun</c>, <c>TrailingBytes</c>, <c>InvalidUtf8</c>,
    /// <c>InvalidUuid</c>, <c>ValueOutOfRange</c>, <c>UnknownTag</c>. Every
    /// message states what was expected, what was found, and where. There are no
    /// silent fallbacks: no null returns, no partial values, no lossy decodes.</para>
    ///
    /// <para><see cref="WireFormatVersion"/> freezes the wire contract; a change
    /// to any rule requires a NEW version, never an edit in place.</para>
    /// </summary>
    public sealed class WireError : Exception
    {
        /// <summary>The frozen wire-format version (the determinism contract).</summary>
        public const string WireFormatVersion = "1";

        /// <summary>The error kind, one of the seven taxonomy names (e.g. <c>"Truncated"</c>).</summary>
        public string Kind { get; }

        /// <summary>
        /// Absolute byte offset where the failing read started (read-side errors);
        /// <c>null</c> when not applicable (writer-side errors).
        /// </summary>
        public int? Offset { get; }

        private WireError(string kind, string message, int? offset)
            : base(message)
        {
            Kind = kind;
            Offset = offset;
        }

        public static WireError Truncated(int needed, int remaining, int offset, string context)
        {
            return new WireError(
                "Truncated",
                $"Truncated: {context} needs {needed} byte(s) at offset {offset}, {remaining} remain",
                offset);
        }

        public static WireError LengthOverrun(int declared, int remaining, int offset, string context)
        {
            return new WireError(
                "LengthOverrun",
                $"LengthOverrun: {context} at offset {offset} declares {declared} byte(s), {remaining} remain after the length field",
                offset);
        }

        public static WireError TrailingBytes(int count, int offset)
        {
            return new WireError(
                "TrailingBytes",
                $"TrailingBytes: expected end of buffer at offset {offset}, {count} byte(s) remain",
                offset);
        }

        public static WireError InvalidUtf8(int? offset, string context)
        {
            return new WireError(
                "InvalidUtf8",
                offset.HasValue
                    ? $"InvalidUtf8: {context} at offset {offset.Value}"
                    : $"InvalidUtf8: {context}",
                offset);
        }

        public static WireError InvalidUuid(string context)
        {
            return new WireError("InvalidUuid", $"InvalidUuid: {context}", null);
        }

        public static WireError ValueOutOfRange(long value, string type)
        {
            return new WireError("ValueOutOfRange", $"ValueOutOfRange: {value} is not a valid {type}", null);
        }

        /// <summary>Constructor for generated union dispatch on an unrecognized tag byte.</summary>
        public static WireError UnknownTag(byte tag, string context)
        {
            return new WireError("UnknownTag", $"UnknownTag: 0x{tag:x2} is not a known {context}", null);
        }
    }
}
