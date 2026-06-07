using System;
using System.Collections.Generic;
using System.Security.Cryptography;
using System.Text;

namespace Kanonak.Canonical
{
    // -- Value model — the typed-value representation the canonical form consumes.
    //    A consumer (or the vector decoder) builds this; CanonicalForm serializes it.

    public abstract class CanonicalValue { }

    /// <summary>A datatype-typed scalar: carrier tag + raw lexical token (canonicalized on serialize).</summary>
    public sealed class TypedScalar : CanonicalValue
    {
        public Carrier Carrier;
        public string Lexical;
        public TypedScalar(Carrier carrier, string lexical) { Carrier = carrier; Lexical = lexical; }
    }

    /// <summary>An untyped / open-world scalar: the raw token, preserved verbatim (the `string` blob).</summary>
    public sealed class RawScalar : CanonicalValue
    {
        public string Token;
        public RawScalar(string token) { Token = token; }
    }

    /// <summary>A reference to an entity, by its full canonical URI.</summary>
    public sealed class Reference : CanonicalValue
    {
        public string Uri;
        public Reference(string uri) { Uri = uri; }
    }

    /// <summary>An embedded node (optional dict-key name + its own statements).</summary>
    public sealed class Embedded : CanonicalValue
    {
        public string Name; // null when the embedded node had no dict-key
        public List<Statement> Statements;
        public Embedded(string name, List<Statement> statements) { Name = name; Statements = statements; }
    }

    /// <summary>An ordered list (source order preserved — lists carry semantic order).</summary>
    public sealed class KList : CanonicalValue
    {
        public List<CanonicalValue> Items;
        public KList(List<CanonicalValue> items) { Items = items; }
    }

    public sealed class Statement
    {
        public string Predicate;
        public CanonicalValue Value;
        public Statement(string predicate, CanonicalValue value) { Predicate = predicate; Value = value; }
    }

    public sealed class Subject
    {
        public string Uri;
        public List<Statement> Statements;
        public Subject(string uri, List<Statement> statements) { Uri = uri; Statements = statements; }
    }

    public sealed class Package
    {
        public List<Subject> Subjects;
        public Package(List<Subject> subjects) { Subjects = subjects; }
    }

    /// <summary>
    /// The frozen canonical structural form + hash. Subjects are ordered by the
    /// UTF-8 byte sequence of their URI, statements by the UTF-8 byte sequence of
    /// the predicate URI, list elements keep source order. The wire form is compact
    /// JSON with RFC 8785 escaping and a fixed per-blob field order. Hashing the
    /// UTF-8 bytes yields the permanent content address.
    /// </summary>
    public static class CanonicalForm
    {
        public const string CanonicalFormVersion = "1";

        public static string Serialize(Package pkg)
        {
            var sb = new StringBuilder();
            sb.Append("{\"subjects\":[");
            var subjects = new List<Subject>(pkg.Subjects);
            subjects.Sort((a, b) => CompareUtf8(a.Uri, b.Uri));
            for (int i = 0; i < subjects.Count; i++)
            {
                if (i > 0) sb.Append(',');
                EmitSubject(sb, subjects[i]);
            }
            sb.Append("]}");
            return sb.ToString();
        }

        public static string Hash(Package pkg)
        {
            string form = Serialize(pkg);
            byte[] bytes = Encoding.UTF8.GetBytes(form);
            using (var sha = SHA256.Create())
            {
                byte[] digest = sha.ComputeHash(bytes);
                var hex = new StringBuilder(digest.Length * 2);
                foreach (byte b in digest) hex.Append(b.ToString("x2"));
                return "sha256:" + hex;
            }
        }

        // -- Internals ----------------------------------------------------------

        private static void EmitSubject(StringBuilder sb, Subject s)
        {
            sb.Append("{\"uri\":");
            EmitJsonString(sb, s.Uri);
            sb.Append(",\"statements\":[");
            EmitStatements(sb, s.Statements);
            sb.Append("]}");
        }

        private static void EmitStatements(StringBuilder sb, List<Statement> stmts)
        {
            var ordered = new List<Statement>(stmts);
            ordered.Sort((a, b) => CompareUtf8(a.Predicate, b.Predicate));
            for (int i = 0; i < ordered.Count; i++)
            {
                if (i > 0) sb.Append(',');
                sb.Append("{\"predicate\":");
                EmitJsonString(sb, ordered[i].Predicate);
                sb.Append(',');
                EmitValueTail(sb, ordered[i].Value);
                sb.Append('}');
            }
        }

        /// <summary>Emit <c>"type":...</c> + the type-specific tail (shared by statements and list items).</summary>
        private static void EmitValueTail(StringBuilder sb, CanonicalValue v)
        {
            switch (v)
            {
                case TypedScalar t:
                    sb.Append("\"type\":\"typed\",\"carrier\":");
                    EmitJsonString(sb, t.Carrier.Tag());
                    sb.Append(",\"value\":");
                    EmitJsonString(sb, Datatypes.CanonicalScalarLexical(t.Carrier, t.Lexical));
                    break;
                case RawScalar r:
                    sb.Append("\"type\":\"string\",\"value\":");
                    EmitJsonString(sb, r.Token);
                    break;
                case Reference rf:
                    sb.Append("\"type\":\"ref\",\"value\":");
                    EmitJsonString(sb, rf.Uri);
                    break;
                case Embedded e:
                    sb.Append("\"type\":\"embedded\"");
                    if (e.Name != null)
                    {
                        sb.Append(",\"name\":");
                        EmitJsonString(sb, e.Name);
                    }
                    sb.Append(",\"statements\":[");
                    EmitStatements(sb, e.Statements);
                    sb.Append(']');
                    break;
                case KList l:
                    sb.Append("\"type\":\"list\",\"items\":[");
                    for (int i = 0; i < l.Items.Count; i++)
                    {
                        if (i > 0) sb.Append(',');
                        sb.Append('{');
                        EmitValueTail(sb, l.Items[i]);
                        sb.Append('}');
                    }
                    sb.Append(']');
                    break;
                default:
                    throw new ArgumentException("canonicalForm: unrecognized value kind " + v.GetType().Name);
            }
        }

        /// <summary>
        /// RFC 8785 (JCS) string escaping, identical to JavaScript's JSON.stringify:
        /// escape <c>"</c>, <c>\</c>, the short C0 escapes, other C0 as <c>\u00xx</c>
        /// (lowercase); everything else (including non-ASCII) emitted raw.
        /// </summary>
        private static void EmitJsonString(StringBuilder sb, string s)
        {
            sb.Append('"');
            foreach (char c in s)
            {
                switch (c)
                {
                    case '"': sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\b': sb.Append("\\b"); break;
                    case '\f': sb.Append("\\f"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        if (c < 0x20) sb.Append("\\u").Append(((int)c).ToString("x4"));
                        else sb.Append(c);
                        break;
                }
            }
            sb.Append('"');
        }

        /// <summary>
        /// Lexicographic comparison by UTF-8 byte sequence (== Unicode code-point
        /// order) — pinned because language native string compares disagree.
        /// </summary>
        private static int CompareUtf8(string a, string b)
        {
            byte[] ab = Encoding.UTF8.GetBytes(a);
            byte[] bb = Encoding.UTF8.GetBytes(b);
            int n = Math.Min(ab.Length, bb.Length);
            for (int i = 0; i < n; i++)
                if (ab[i] != bb[i]) return ab[i] - bb[i];
            return ab.Length - bb.Length;
        }
    }
}
