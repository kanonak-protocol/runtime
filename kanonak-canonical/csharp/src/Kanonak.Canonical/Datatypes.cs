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
            return EcmaScriptDoubleToken(n);
        }

        /// <summary>
        /// ECMAScript's Number::toString of a finite, non-zero double: RFC 8785's number notation, plain while
        /// -6 &lt; n &lt;= 21. "R" gives the shortest round-tripping digits on .NET Core 3.0 and later; only the
        /// notation is re-derived.
        /// </summary>
        private static string EcmaScriptDoubleToken(double d)
        {
            string sign = d < 0 ? "-" : "";
            string shortest = Math.Abs(d).ToString("R", CultureInfo.InvariantCulture);
            int e = shortest.IndexOf('E');
            string mantissa = e < 0 ? shortest : shortest.Substring(0, e);
            int exponent = e < 0 ? 0 : int.Parse(shortest.Substring(e + 1), NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture);
            int dot = mantissa.IndexOf('.');
            string intPart = dot < 0 ? mantissa : mantissa.Substring(0, dot);
            string fracPart = dot < 0 ? "" : mantissa.Substring(dot + 1);
            string combined = intPart + fracPart;
            string trimmed = combined.TrimStart('0');
            int lead = combined.Length - trimmed.Length;
            string digits = trimmed.TrimEnd('0');
            int n = intPart.Length - lead + exponent;
            int k = digits.Length;
            string body;
            if (k <= n && n <= 21)
                body = digits + new string('0', n - k);
            else if (0 < n && n <= 21)
                body = digits.Substring(0, n) + "." + digits.Substring(n);
            else if (-6 < n && n <= 0)
                body = "0." + new string('0', -n) + digits;
            else
            {
                int exp = n - 1;
                string head = k == 1 ? digits : digits.Substring(0, 1) + "." + digits.Substring(1);
                body = head + "e" + (exp >= 0 ? "+" : "-") + Math.Abs(exp).ToString(CultureInfo.InvariantCulture);
            }
            return sign + body;
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

        // -- Temporal carriers: XSD 1.1's rules, on the proleptic Gregorian calendar with a year 0. --
        // A year of any length is an exact BigInteger, so a UTC shift across a year boundary has no range limit
        // (runtime#36).

        private static string Pad2(int n) => n < 10 ? "0" + n : n.ToString(CultureInfo.InvariantCulture);

        private static readonly int[] DaysInMonthTable = { 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31 };

        private static bool IsLeapYear(BigInteger year)
        {
            int r = (int)(((year % 400) + 400) % 400);
            return r % 4 == 0 && (r % 100 != 0 || r == 0);
        }

        private static int DaysInMonth(BigInteger year, int month) =>
            month == 2 && IsLeapYear(year) ? 29 : DaysInMonthTable[month - 1];

        /// <summary>A year at least four digits wide, its sign kept; year 0 has none.</summary>
        private static string YearText(BigInteger year)
        {
            string digits = BigInteger.Abs(year).ToString(CultureInfo.InvariantCulture).PadLeft(4, '0');
            return (year.Sign < 0 ? "-" : "") + digits;
        }

        private sealed class Civil
        {
            public readonly BigInteger Year;
            public readonly int Month;
            public readonly int Day;

            public Civil(BigInteger year, int month, int day)
            {
                Year = year;
                Month = month;
                Day = day;
            }

            public Civil NextDay()
            {
                if (Day < DaysInMonth(Year, Month)) return new Civil(Year, Month, Day + 1);
                if (Month < 12) return new Civil(Year, Month + 1, 1);
                return new Civil(Year + 1, 1, 1);
            }

            public Civil PreviousDay()
            {
                if (Day > 1) return new Civil(Year, Month, Day - 1);
                if (Month > 1) return new Civil(Year, Month - 1, DaysInMonth(Year, Month - 1));
                return new Civil(Year - 1, 12, 31);
            }
        }

        private static int TwoDigits(string field) => int.Parse(field, CultureInfo.InvariantCulture);

        private static Civil CivilOf(string yy, string mo, string dd, string what, string raw)
        {
            BigInteger year = BigInteger.Parse(yy, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture);
            int month = TwoDigits(mo), day = TwoDigits(dd);
            if (month < 1 || month > 12)
                throw new FormatException($"{what}: '{raw}' has month {mo}; a month is 01 to 12");
            if (day < 1 || day > DaysInMonth(year, month))
                throw new FormatException($"{what}: '{raw}' has day {dd}, which month {mo} of year {YearText(year)} doesn't have");
            return new Civil(year, month, day);
        }

        /// <summary>Minutes east of UTC, and 0 with no offset. XSD 1.1 bounds an offset at 14:00.</summary>
        private static int OffsetMinutes(string tz, string what, string raw)
        {
            if (string.IsNullOrEmpty(tz) || tz == "Z") return 0;
            int hours = TwoDigits(tz.Substring(1, 2)), minutes = TwoDigits(tz.Substring(4, 2));
            if (minutes > 59 || hours > 14 || (hours == 14 && minutes > 0))
                throw new FormatException($"{what}: '{raw}' has offset {tz}; an offset is at most 14:00 either way");
            return (tz[0] == '-' ? -1 : 1) * (hours * 60 + minutes);
        }

        /// <summary>XSD 1.1: no 60th second, and 24:00:00 only as the first instant of the next day.</summary>
        private static void ClockOf(string hh, string mi, string ss, string fraction, string what, string raw, out int hour, out int minute)
        {
            hour = TwoDigits(hh);
            minute = TwoDigits(mi);
            int second = TwoDigits(ss);
            bool endOfDay = hour == 24 && minute == 0 && second == 0 && fraction == "";
            if (minute > 59 || second > 59 || (hour > 23 && !endOfDay))
                throw new FormatException($"{what}: '{raw}' has time {hh}:{mi}:{ss}{fraction}; XSD 1.1 allows 00:00:00 to 23:59:59, and 24:00:00 exactly");
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

        public static string CanonicalDateTime(string raw)
        {
            const string what = "canonicalDateTime";
            Match m = DateTimeRe.Match(raw.Trim());
            if (!m.Success)
                throw new FormatException($"{what}: '{raw}' is not a valid xsd:dateTime lexical");
            string ss = m.Groups[6].Value;
            string fraction = CanonicalFraction(m.Groups[7].Value);
            string tz = m.Groups[8].Value;
            Civil civil = CivilOf(m.Groups[1].Value, m.Groups[2].Value, m.Groups[3].Value, what, raw);
            ClockOf(m.Groups[4].Value, m.Groups[5].Value, ss, fraction, what, raw, out int hour, out int minute);
            int minutes = hour * 60 + minute - OffsetMinutes(tz, what, raw);
            if (minutes < 0)
            {
                minutes += 1440;
                civil = civil.PreviousDay();
            }
            else if (minutes >= 1440)
            {
                minutes -= 1440;
                civil = civil.NextDay();
            }
            string zone = string.IsNullOrEmpty(tz) ? "" : "Z";
            return $"{YearText(civil.Year)}-{Pad2(civil.Month)}-{Pad2(civil.Day)}T{Pad2(minutes / 60)}:{Pad2(minutes % 60)}:{ss}{fraction}{zone}";
        }

        public static string CanonicalDate(string raw)
        {
            const string what = "canonicalDate";
            Match m = DateRe.Match(raw.Trim());
            if (!m.Success)
                throw new FormatException($"{what}: '{raw}' is not a valid xsd:date lexical");
            Civil civil = CivilOf(m.Groups[1].Value, m.Groups[2].Value, m.Groups[3].Value, what, raw);
            OffsetMinutes(m.Groups[4].Value, what, raw);
            return $"{YearText(civil.Year)}-{m.Groups[2].Value}-{m.Groups[3].Value}{CanonicalTz(m.Groups[4].Value)}";
        }

        public static string CanonicalTime(string raw)
        {
            const string what = "canonicalTime";
            Match m = TimeRe.Match(raw.Trim());
            if (!m.Success)
                throw new FormatException($"{what}: '{raw}' is not a valid xsd:time lexical");
            string hh = m.Groups[1].Value, mi = m.Groups[2].Value, ss = m.Groups[3].Value;
            string fraction = CanonicalFraction(m.Groups[4].Value);
            ClockOf(hh, mi, ss, fraction, what, raw, out int hour, out _);
            OffsetMinutes(m.Groups[5].Value, what, raw);
            return $"{(hour == 24 ? "00" : hh)}:{mi}:{ss}{fraction}{CanonicalTz(m.Groups[5].Value)}";
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
                case Carrier.LangString: throw new FormatException($"canonicalLangString: a language string is a structured value, an embedded {{ lexicalForm, language }}, so its tag is hashed with it; the langString carrier has no slot for the tag and refuses the scalar '{raw}'");
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
