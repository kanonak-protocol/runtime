using System;
using System.Globalization;
using System.Numerics;
using System.Text;
using System.Text.RegularExpressions;

namespace Kanonak.Canonical
{
    /// <summary>
    /// Per-carrier canonical lexical forms — the normative heart of the canonical
    /// form. Pure functions, no dependence on a parser or object model, frozen to
    /// canonicalFormVersion "1". Each function throws on malformed input (fail loud).
    /// Verified byte-for-byte against the golden lexical vectors.
    /// </summary>
    public static class Datatypes
    {
        private static readonly Regex IntegerRe = new Regex(@"^[+-]?\d+$", RegexOptions.Compiled);
        private static readonly Regex DecimalRe = new Regex(@"^([+-]?)(\d*)(?:\.(\d*))?$", RegexOptions.Compiled);
        private static readonly Regex IeeeRe = new Regex(@"^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$", RegexOptions.Compiled);
        private static readonly Regex HexRe = new Regex(@"^([0-9A-Fa-f]{2})*$", RegexOptions.Compiled);
        private static readonly Regex Base64Re = new Regex(@"^[A-Za-z0-9+/]*={0,2}$", RegexOptions.Compiled);
        private static readonly Regex DateTimeRe = new Regex(@"^(-?\d{4,})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$", RegexOptions.Compiled);
        private static readonly Regex DateRe = new Regex(@"^(-?\d{4,})-(\d{2})-(\d{2})(Z|[+-]\d{2}:\d{2})?$", RegexOptions.Compiled);
        private static readonly Regex TimeRe = new Regex(@"^(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})?$", RegexOptions.Compiled);

        // -- Integer carrier — arbitrary precision (BigInteger). ----------------

        public static string CanonicalInteger(string raw)
        {
            string token = raw.Trim();
            if (!IntegerRe.IsMatch(token))
                throw new FormatException($"canonicalInteger: '{raw}' is not a valid xsd:integer lexical");
            if (token.Length > 0 && token[0] == '+') token = token.Substring(1);
            return BigInteger.Parse(token, CultureInfo.InvariantCulture).ToString(CultureInfo.InvariantCulture);
        }

        // -- Decimal carrier — arbitrary precision, string arithmetic. ----------

        public static string CanonicalDecimal(string raw)
        {
            string token = raw.Trim();
            Match m = DecimalRe.Match(token);
            string intRaw = m.Success ? m.Groups[2].Value : "";
            string fracRaw = m.Success && m.Groups[3].Success ? m.Groups[3].Value : "";
            if (!m.Success || (intRaw == "" && fracRaw == ""))
                throw new FormatException($"canonicalDecimal: '{raw}' is not a valid xsd:decimal lexical");
            string sign = m.Groups[1].Value == "-" ? "-" : "";
            string intPart = intRaw.TrimStart('0');
            if (intPart == "") intPart = "0";
            string fracPart = fracRaw.TrimEnd('0');
            string magnitude = fracPart.Length > 0 ? intPart + "." + fracPart : intPart;
            if (magnitude == "0") return "0";
            return sign + magnitude;
        }

        // -- IEEE carriers — double / float. ------------------------------------

        private static string CanonicalIeee(string raw, bool single, string label)
        {
            string token = raw.Trim();
            if (token == "NaN") return "NaN";
            if (token == "INF") return "INF";
            if (token == "-INF") return "-INF";
            if (!IeeeRe.IsMatch(token))
                throw new FormatException($"canonical{label}: '{raw}' is not a valid xsd:{label.ToLowerInvariant()} lexical");
            double n = double.Parse(token, NumberStyles.Float, CultureInfo.InvariantCulture);
            if (single) n = (float)n;
            if (double.IsNaN(n) || double.IsInfinity(n))
                throw new FormatException($"canonical{label}: '{raw}' is out of the finite range");
            if (n == 0.0) return "0"; // collapses -0
            // .NET Core 3.0+ default ToString is the shortest round-tripping decimal.
            return n.ToString(CultureInfo.InvariantCulture);
        }

        public static string CanonicalDouble(string raw) => CanonicalIeee(raw, false, "Double");

        public static string CanonicalFloat(string raw) => CanonicalIeee(raw, true, "Float");

        // -- Boolean carrier. ---------------------------------------------------

        public static string CanonicalBoolean(string raw)
        {
            string token = raw.Trim();
            if (token == "true" || token == "1") return "true";
            if (token == "false" || token == "0") return "false";
            throw new FormatException($"canonicalBoolean: '{raw}' is not a valid xsd:boolean lexical");
        }

        // -- String carriers — NFC, no whitespace collapse. ---------------------

        public static string CanonicalString(string raw) => raw.Normalize(NormalizationForm.FormC);

        public static string CanonicalAnyUri(string raw) => raw.Normalize(NormalizationForm.FormC);

        /// <summary>BCP 47 canonical case by subtag position.</summary>
        public static string CanonicalLanguageTag(string tag)
        {
            string[] subtags = tag.Trim().Split('-');
            if (subtags.Length == 0 || subtags[0] == "")
                throw new FormatException($"canonicalLanguageTag: '{tag}' is not a valid language tag");
            for (int i = 0; i < subtags.Length; i++)
            {
                string sub = subtags[i];
                if (i == 0) { subtags[i] = sub.ToLowerInvariant(); continue; }
                if (sub.Length == 4 && IsAlpha(sub))
                    subtags[i] = char.ToUpperInvariant(sub[0]) + sub.Substring(1).ToLowerInvariant(); // script Titlecase
                else if (sub.Length == 2 && IsAlpha(sub))
                    subtags[i] = sub.ToUpperInvariant(); // region UPPER
                else
                    subtags[i] = sub.ToLowerInvariant();
            }
            return string.Join("-", subtags);
        }

        private static bool IsAlpha(string s)
        {
            foreach (char c in s)
                if (!((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z'))) return false;
            return true;
        }

        // -- Binary carriers. ---------------------------------------------------

        public static string CanonicalHexBinary(string raw)
        {
            string token = raw.Trim();
            if (!HexRe.IsMatch(token))
                throw new FormatException($"canonicalHexBinary: '{raw}' is not a valid xsd:hexBinary lexical");
            return token.ToUpperInvariant();
        }

        public static string CanonicalBase64(string raw)
        {
            string stripped = Regex.Replace(raw, @"\s+", "");
            if (!Base64Re.IsMatch(stripped) || stripped.Length % 4 != 0)
                throw new FormatException($"canonicalBase64: '{raw}' is not a valid xsd:base64Binary lexical");
            byte[] bytes = Convert.FromBase64String(stripped);
            return Convert.ToBase64String(bytes);
        }

        // -- Temporal carriers. -------------------------------------------------

        private static string Pad2(int n) => n < 10 ? "0" + n : n.ToString(CultureInfo.InvariantCulture);

        private static string CanonicalYear(string raw)
        {
            bool neg = raw.StartsWith("-", StringComparison.Ordinal);
            string digits = (neg ? raw.Substring(1) : raw).TrimStart('0');
            if (digits == "") digits = "0";
            string padded = digits.Length < 4 ? digits.PadLeft(4, '0') : digits;
            return (neg ? "-" : "") + padded;
        }

        private static string CanonicalFraction(string frac)
        {
            if (string.IsNullOrEmpty(frac)) return "";
            string trimmed = frac.TrimStart('.').TrimEnd('0');
            return trimmed.Length > 0 ? "." + trimmed : "";
        }

        private static string CanonicalTz(string tz)
        {
            if (string.IsNullOrEmpty(tz)) return "";
            if (tz == "Z" || tz == "+00:00" || tz == "-00:00") return "Z";
            return tz;
        }

        private static int TzOffsetMinutes(string tz)
        {
            if (tz == "Z") return 0;
            int sign = tz[0] == '-' ? -1 : 1;
            int hh = int.Parse(tz.Substring(1, 2), CultureInfo.InvariantCulture);
            int mm = int.Parse(tz.Substring(4, 2), CultureInfo.InvariantCulture);
            return sign * (hh * 60 + mm);
        }

        public static string CanonicalDateTime(string raw)
        {
            Match m = DateTimeRe.Match(raw.Trim());
            if (!m.Success)
                throw new FormatException($"canonicalDateTime: '{raw}' is not a valid xsd:dateTime lexical");
            string yy = m.Groups[1].Value, mo = m.Groups[2].Value, dd = m.Groups[3].Value;
            string hh = m.Groups[4].Value, mi = m.Groups[5].Value, ss = m.Groups[6].Value;
            string fraction = CanonicalFraction(m.Groups[7].Value);
            string tz = m.Groups[8].Value;

            if (string.IsNullOrEmpty(tz))
                return $"{CanonicalYear(yy)}-{mo}-{dd}T{hh}:{mi}:{ss}{fraction}";

            // Shift integer fields to UTC on the proleptic Gregorian calendar.
            var dt = new DateTime(
                int.Parse(yy, CultureInfo.InvariantCulture),
                int.Parse(mo, CultureInfo.InvariantCulture),
                int.Parse(dd, CultureInfo.InvariantCulture),
                int.Parse(hh, CultureInfo.InvariantCulture),
                int.Parse(mi, CultureInfo.InvariantCulture),
                int.Parse(ss, CultureInfo.InvariantCulture),
                DateTimeKind.Utc);
            var shifted = dt.AddMinutes(-TzOffsetMinutes(tz));
            string y = CanonicalYear(shifted.Year.ToString(CultureInfo.InvariantCulture));
            return $"{y}-{Pad2(shifted.Month)}-{Pad2(shifted.Day)}T{Pad2(shifted.Hour)}:{Pad2(shifted.Minute)}:{Pad2(shifted.Second)}{fraction}Z";
        }

        public static string CanonicalDate(string raw)
        {
            Match m = DateRe.Match(raw.Trim());
            if (!m.Success)
                throw new FormatException($"canonicalDate: '{raw}' is not a valid xsd:date lexical");
            return $"{CanonicalYear(m.Groups[1].Value)}-{m.Groups[2].Value}-{m.Groups[3].Value}{CanonicalTz(m.Groups[4].Value)}";
        }

        public static string CanonicalTime(string raw)
        {
            Match m = TimeRe.Match(raw.Trim());
            if (!m.Success)
                throw new FormatException($"canonicalTime: '{raw}' is not a valid xsd:time lexical");
            string hh = m.Groups[1].Value, mi = m.Groups[2].Value, ss = m.Groups[3].Value;
            string fraction = CanonicalFraction(m.Groups[4].Value);
            if (hh == "24" && mi == "00" && ss == "00" && fraction == "") hh = "00";
            return $"{hh}:{mi}:{ss}{fraction}{CanonicalTz(m.Groups[5].Value)}";
        }

        // -- Dispatch. ----------------------------------------------------------

        /// <summary>Canonical lexical form of a raw token under a carrier. Throws on malformed input.</summary>
        public static string CanonicalScalarLexical(Carrier carrier, string raw)
        {
            switch (carrier)
            {
                case Carrier.Integer: return CanonicalInteger(raw);
                case Carrier.Decimal: return CanonicalDecimal(raw);
                case Carrier.Double: return CanonicalDouble(raw);
                case Carrier.Float: return CanonicalFloat(raw);
                case Carrier.Boolean: return CanonicalBoolean(raw);
                case Carrier.String: return CanonicalString(raw);
                case Carrier.AnyUri: return CanonicalAnyUri(raw);
                case Carrier.LangString: return CanonicalString(raw);
                case Carrier.DateTime: return CanonicalDateTime(raw);
                case Carrier.Date: return CanonicalDate(raw);
                case Carrier.Time: return CanonicalTime(raw);
                case Carrier.HexBinary: return CanonicalHexBinary(raw);
                case Carrier.Base64Binary: return CanonicalBase64(raw);
                default: throw new ArgumentOutOfRangeException(nameof(carrier));
            }
        }
    }
}
