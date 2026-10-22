package org.kanonak.canonical;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.math.MathContext;
import java.math.RoundingMode;
import java.text.Normalizer;
import java.util.Base64;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Per-carrier canonical lexical forms — pure functions, JDK-only, frozen to
 * canonicalFormVersion "1". Each throws {@link IllegalArgumentException} on
 * malformed input. Verified byte-for-byte against the golden lexical vectors.
 */
public final class Datatypes {
    private Datatypes() {}

    private static final Pattern INTEGER = Pattern.compile("^[+-]?\\d+$");
    private static final Pattern DECIMAL = Pattern.compile("^([+-]?)(\\d*)(?:\\.(\\d*))?$");
    private static final Pattern IEEE = Pattern.compile("^[+-]?(\\d+\\.?\\d*|\\.\\d+)([eE][+-]?\\d+)?$");
    private static final Pattern HEX = Pattern.compile("^([0-9A-Fa-f]{2})*$");
    private static final Pattern BASE64 = Pattern.compile("^[A-Za-z0-9+/]*={0,2}$");
    private static final Pattern DATE_TIME = Pattern.compile("^(-?\\d{4,})-(\\d{2})-(\\d{2})T(\\d{2}):(\\d{2}):(\\d{2})(\\.\\d+)?(Z|[+-]\\d{2}:\\d{2})?$");
    private static final Pattern DATE = Pattern.compile("^(-?\\d{4,})-(\\d{2})-(\\d{2})(Z|[+-]\\d{2}:\\d{2})?$");
    private static final Pattern TIME = Pattern.compile("^(\\d{2}):(\\d{2}):(\\d{2})(\\.\\d+)?(Z|[+-]\\d{2}:\\d{2})?$");

    public static String canonicalInteger(String raw) {
        String t = raw.trim();
        if (!INTEGER.matcher(t).matches())
            throw new IllegalArgumentException("canonicalInteger: '" + raw + "' invalid");
        String sign = "", digits = t;
        if (t.startsWith("-")) { sign = "-"; digits = t.substring(1); }
        else if (t.startsWith("+")) { digits = t.substring(1); }
        String stripped = stripLeading(digits, '0');
        if (stripped.isEmpty()) stripped = "0";
        if (stripped.equals("0")) return "0";
        return sign + stripped;
    }

    public static String canonicalDecimal(String raw) {
        String t = raw.trim();
        Matcher m = DECIMAL.matcher(t);
        String intRaw = "", fracRaw = "";
        boolean ok = m.matches();
        if (ok) {
            intRaw = m.group(2) == null ? "" : m.group(2);
            fracRaw = m.group(3) == null ? "" : m.group(3);
        }
        if (!ok || (intRaw.isEmpty() && fracRaw.isEmpty()))
            throw new IllegalArgumentException("canonicalDecimal: '" + raw + "' invalid");
        String sign = "-".equals(m.group(1)) ? "-" : "";
        String intPart = stripLeading(intRaw, '0');
        if (intPart.isEmpty()) intPart = "0";
        String fracPart = stripTrailing(fracRaw, '0');
        String magnitude = !fracPart.isEmpty() ? intPart + "." + fracPart : intPart;
        if (magnitude.equals("0")) return "0";
        return sign + magnitude;
    }

    private static String canonicalIeee(String raw, boolean single, String label) {
        String t = raw.trim();
        if (t.equals("NaN")) return "NaN";
        if (t.equals("INF")) return "INF";
        if (t.equals("-INF")) return "-INF";
        if (!IEEE.matcher(t).matches())
            throw new IllegalArgumentException("canonical" + label + ": '" + raw + "' invalid");
        double n = Double.parseDouble(t);
        // A float is its value rounded to 32 bits, then written as that double (runtime#35).
        double value = single ? (double) (float) n : n;
        if (Double.isNaN(value) || Double.isInfinite(value))
            throw new IllegalArgumentException("canonical" + label + ": '" + raw + "' out of range");
        if (value == 0.0) return "0";
        return ecmaScriptDoubleToken(value);
    }

    /**
     * ECMAScript's Number::toString of a finite, non-zero double: RFC 8785's number notation, plain while
     * -6 &lt; n &lt;= 21. The digits are searched for here, since Double.toString is not the shortest
     * round-tripping decimal before JDK 19.
     */
    private static String ecmaScriptDoubleToken(double d) {
        String sign = d < 0 ? "-" : "";
        BigDecimal shortest = shortestRoundTrip(Math.abs(d));
        String digits = shortest.unscaledValue().toString();
        int k = digits.length();
        int n = k - shortest.scale();
        String body;
        if (k <= n && n <= 21) {
            body = digits + "0".repeat(n - k);
        } else if (0 < n && n <= 21) {
            body = digits.substring(0, n) + "." + digits.substring(n);
        } else if (-6 < n && n <= 0) {
            body = "0." + "0".repeat(-n) + digits;
        } else {
            int e = n - 1;
            String head = k == 1 ? digits : digits.charAt(0) + "." + digits.substring(1);
            body = head + "e" + (e >= 0 ? "+" : "-") + Math.abs(e);
        }
        return sign + body;
    }

    /**
     * The fewest significant digits that read back as {@code d}, and of those the closest to its exact value,
     * the even one on a tie: ECMAScript's choice. The rounded value's neighbours are tried too, since a power of
     * two has a narrower gap below it than above.
     */
    private static BigDecimal shortestRoundTrip(double d) {
        BigDecimal exact = new BigDecimal(d);
        for (int precision = 1; precision <= 17; precision++) {
            BigDecimal rounded = exact.round(new MathContext(precision, RoundingMode.HALF_EVEN));
            BigDecimal ulp = rounded.ulp();
            BigDecimal best = null;
            for (BigDecimal candidate : new BigDecimal[] { rounded.subtract(ulp), rounded, rounded.add(ulp) }) {
                if (candidate.signum() <= 0 || Double.parseDouble(candidate.toString()) != d) continue;
                if (best == null) { best = candidate; continue; }
                int closer = candidate.subtract(exact).abs().compareTo(best.subtract(exact).abs());
                if (closer < 0 || (closer == 0 && !candidate.unscaledValue().testBit(0))) best = candidate;
            }
            if (best != null) return best.stripTrailingZeros();
        }
        throw new IllegalStateException("no decimal of at most 17 significant digits reads back as " + d);
    }

    public static String canonicalDouble(String raw) { return canonicalIeee(raw, false, "Double"); }
    public static String canonicalFloat(String raw) { return canonicalIeee(raw, true, "Float"); }

    public static String canonicalBoolean(String raw) {
        String t = raw.trim();
        if (t.equals("true") || t.equals("1")) return "true";
        if (t.equals("false") || t.equals("0")) return "false";
        throw new IllegalArgumentException("canonicalBoolean: '" + raw + "' invalid");
    }

    public static String canonicalString(String raw) {
        return Normalizer.normalize(raw, Normalizer.Form.NFC);
    }

    public static String canonicalLanguageTag(String tag) {
        String[] subs = tag.trim().split("-", -1);
        if (subs.length == 0 || subs[0].isEmpty())
            throw new IllegalArgumentException("canonicalLanguageTag: '" + tag + "' invalid");
        StringBuilder out = new StringBuilder();
        for (int i = 0; i < subs.length; i++) {
            if (i > 0) out.append('-');
            String sub = subs[i];
            if (i == 0) out.append(sub.toLowerCase());
            else if (sub.length() == 4 && isAlpha(sub))
                out.append(Character.toUpperCase(sub.charAt(0))).append(sub.substring(1).toLowerCase());
            else if (sub.length() == 2 && isAlpha(sub)) out.append(sub.toUpperCase());
            else out.append(sub.toLowerCase());
        }
        return out.toString();
    }

    public static String canonicalHexBinary(String raw) {
        String t = raw.trim();
        if (!HEX.matcher(t).matches())
            throw new IllegalArgumentException("canonicalHexBinary: '" + raw + "' invalid");
        return t.toUpperCase();
    }

    public static String canonicalBase64(String raw) {
        String stripped = raw.replaceAll("\\s+", "");
        if (!BASE64.matcher(stripped).matches() || stripped.length() % 4 != 0)
            throw new IllegalArgumentException("canonicalBase64: '" + raw + "' invalid");
        byte[] bytes = Base64.getDecoder().decode(stripped);
        return Base64.getEncoder().encodeToString(bytes);
    }

    // -- temporal: XSD 1.1's rules, on the proleptic Gregorian calendar with a year 0 --
    // A year of any length is an exact BigInteger, so a UTC shift across a year boundary has no range limit
    // (runtime#36).

    public static String canonicalDateTime(String raw) {
        String what = "canonicalDateTime";
        Matcher m = DATE_TIME.matcher(raw.trim());
        if (!m.matches())
            throw new IllegalArgumentException(what + ": '" + raw + "' invalid");
        String fraction = canonicalFraction(m.group(7));
        String tz = m.group(8);
        Civil civil = civilOf(m.group(1), m.group(2), m.group(3), what, raw);
        int[] clock = clockOf(m.group(4), m.group(5), m.group(6), fraction, what, raw);
        int minutes = clock[0] * 60 + clock[1] - offsetMinutes(tz, what, raw);
        if (minutes < 0) {
            minutes += 1440;
            civil = civil.previousDay();
        } else if (minutes >= 1440) {
            minutes -= 1440;
            civil = civil.nextDay();
        }
        return yearText(civil.year) + "-" + pad2(civil.month) + "-" + pad2(civil.day)
            + "T" + pad2(minutes / 60) + ":" + pad2(minutes % 60) + ":" + m.group(6) + fraction
            + (tz == null || tz.isEmpty() ? "" : "Z");
    }

    public static String canonicalDate(String raw) {
        String what = "canonicalDate";
        Matcher m = DATE.matcher(raw.trim());
        if (!m.matches())
            throw new IllegalArgumentException(what + ": '" + raw + "' invalid");
        Civil civil = civilOf(m.group(1), m.group(2), m.group(3), what, raw);
        offsetMinutes(m.group(4), what, raw);
        return yearText(civil.year) + "-" + m.group(2) + "-" + m.group(3) + canonicalTz(m.group(4));
    }

    public static String canonicalTime(String raw) {
        String what = "canonicalTime";
        Matcher m = TIME.matcher(raw.trim());
        if (!m.matches())
            throw new IllegalArgumentException(what + ": '" + raw + "' invalid");
        String fraction = canonicalFraction(m.group(4));
        int[] clock = clockOf(m.group(1), m.group(2), m.group(3), fraction, what, raw);
        offsetMinutes(m.group(5), what, raw);
        String hh = clock[0] == 24 ? "00" : m.group(1);
        return hh + ":" + m.group(2) + ":" + m.group(3) + fraction + canonicalTz(m.group(5));
    }

    private static final int[] DAYS_IN_MONTH = { 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31 };
    private static final BigInteger FOUR_HUNDRED = BigInteger.valueOf(400);

    private static boolean isLeapYear(BigInteger year) {
        int r = year.mod(FOUR_HUNDRED).intValue();
        return r % 4 == 0 && (r % 100 != 0 || r == 0);
    }

    private static int daysInMonth(BigInteger year, int month) {
        return month == 2 && isLeapYear(year) ? 29 : DAYS_IN_MONTH[month - 1];
    }

    /** A year at least four digits wide, its sign kept; year 0 has none. */
    private static String yearText(BigInteger year) {
        String digits = year.abs().toString();
        while (digits.length() < 4) digits = "0" + digits;
        return (year.signum() < 0 ? "-" : "") + digits;
    }

    private static final class Civil {
        final BigInteger year;
        final int month;
        final int day;

        Civil(BigInteger year, int month, int day) {
            this.year = year;
            this.month = month;
            this.day = day;
        }

        Civil nextDay() {
            if (day < daysInMonth(year, month)) return new Civil(year, month, day + 1);
            if (month < 12) return new Civil(year, month + 1, 1);
            return new Civil(year.add(BigInteger.ONE), 1, 1);
        }

        Civil previousDay() {
            if (day > 1) return new Civil(year, month, day - 1);
            if (month > 1) return new Civil(year, month - 1, daysInMonth(year, month - 1));
            return new Civil(year.subtract(BigInteger.ONE), 12, 31);
        }
    }

    private static Civil civilOf(String yy, String mo, String dd, String what, String raw) {
        BigInteger year = new BigInteger(yy);
        int month = Integer.parseInt(mo);
        int day = Integer.parseInt(dd);
        if (month < 1 || month > 12)
            throw new IllegalArgumentException(what + ": '" + raw + "' has month " + mo + "; a month is 01 to 12");
        if (day < 1 || day > daysInMonth(year, month))
            throw new IllegalArgumentException(what + ": '" + raw + "' has day " + dd + ", which month " + mo
                + " of year " + yearText(year) + " doesn't have");
        return new Civil(year, month, day);
    }

    /** Minutes east of UTC, and 0 with no offset. XSD 1.1 bounds an offset at 14:00. */
    private static int offsetMinutes(String tz, String what, String raw) {
        if (tz == null || tz.isEmpty() || tz.equals("Z")) return 0;
        int hours = Integer.parseInt(tz.substring(1, 3));
        int minutes = Integer.parseInt(tz.substring(4, 6));
        if (minutes > 59 || hours > 14 || (hours == 14 && minutes > 0))
            throw new IllegalArgumentException(what + ": '" + raw + "' has offset " + tz + "; an offset is at most 14:00 either way");
        return (tz.charAt(0) == '-' ? -1 : 1) * (hours * 60 + minutes);
    }

    /** XSD 1.1: no 60th second, and 24:00:00 only as the first instant of the next day. Returns {hour, minute}. */
    private static int[] clockOf(String hh, String mi, String ss, String fraction, String what, String raw) {
        int hour = Integer.parseInt(hh);
        int minute = Integer.parseInt(mi);
        int second = Integer.parseInt(ss);
        boolean endOfDay = hour == 24 && minute == 0 && second == 0 && fraction.isEmpty();
        if (minute > 59 || second > 59 || (hour > 23 && !endOfDay))
            throw new IllegalArgumentException(what + ": '" + raw + "' has time " + hh + ":" + mi + ":" + ss + fraction
                + "; XSD 1.1 allows 00:00:00 to 23:59:59, and 24:00:00 exactly");
        return new int[] { hour, minute };
    }

    public static String canonicalScalarLexical(Carrier c, String raw) {
        switch (c) {
            case INTEGER: return canonicalInteger(raw);
            case DECIMAL: return canonicalDecimal(raw);
            case DOUBLE: return canonicalDouble(raw);
            case FLOAT: return canonicalFloat(raw);
            case BOOLEAN: return canonicalBoolean(raw);
            case STRING: case ANY_URI: return canonicalString(raw);
            case LANG_STRING: throw new IllegalArgumentException("canonicalLangString: a language string is a structured value, an embedded { lexicalForm, language }, so its tag is hashed with it; the langString carrier has no slot for the tag and refuses the scalar '" + raw + "'");
            case DATE_TIME: return canonicalDateTime(raw);
            case DATE: return canonicalDate(raw);
            case TIME: return canonicalTime(raw);
            case HEX_BINARY: return canonicalHexBinary(raw);
            case BASE64_BINARY: return canonicalBase64(raw);
            default: throw new IllegalArgumentException("unknown carrier " + c);
        }
    }

    // -- helpers ------------------------------------------------------------

    private static String canonicalFraction(String frac) {
        if (frac == null || frac.isEmpty()) return "";
        String t = stripTrailing(frac.startsWith(".") ? frac.substring(1) : frac, '0');
        return t.isEmpty() ? "" : "." + t;
    }

    private static String canonicalTz(String tz) {
        if (tz == null || tz.isEmpty()) return "";
        if (tz.equals("Z") || tz.equals("+00:00") || tz.equals("-00:00")) return "Z";
        return tz;
    }

    private static String pad2(int n) { return n < 10 ? "0" + n : Integer.toString(n); }

    private static boolean isAlpha(String s) {
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (!((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z'))) return false;
        }
        return true;
    }

    private static String stripLeading(String s, char c) {
        int i = 0;
        while (i < s.length() && s.charAt(i) == c) i++;
        return s.substring(i);
    }

    private static String stripTrailing(String s, char c) {
        int i = s.length();
        while (i > 0 && s.charAt(i - 1) == c) i--;
        return s.substring(0, i);
    }
}
