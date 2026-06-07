package org.kanonak.canonical;

import java.text.Normalizer;
import java.time.LocalDateTime;
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
        if (single) {
            float f = (float) n;
            if (Float.isNaN(f) || Float.isInfinite(f))
                throw new IllegalArgumentException("canonical" + label + ": '" + raw + "' out of range");
            if (f == 0.0f) return "0";
            return shortest(Float.toString(f));
        }
        if (Double.isNaN(n) || Double.isInfinite(n))
            throw new IllegalArgumentException("canonical" + label + ": '" + raw + "' out of range");
        if (n == 0.0) return "0";
        return shortest(Double.toString(n));
    }

    /** Java prints whole values as "1000.0"; the canonical form drops the ".0". */
    private static String shortest(String s) {
        return s.endsWith(".0") ? s.substring(0, s.length() - 2) : s;
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

    // -- temporal -----------------------------------------------------------

    public static String canonicalDateTime(String raw) {
        Matcher m = DATE_TIME.matcher(raw.trim());
        if (!m.matches())
            throw new IllegalArgumentException("canonicalDateTime: '" + raw + "' invalid");
        String fraction = canonicalFraction(m.group(7));
        String tz = m.group(8);
        if (tz == null || tz.isEmpty())
            return canonicalYear(m.group(1)) + "-" + m.group(2) + "-" + m.group(3)
                + "T" + m.group(4) + ":" + m.group(5) + ":" + m.group(6) + fraction;
        LocalDateTime dt = LocalDateTime.of(
            Integer.parseInt(m.group(1)), Integer.parseInt(m.group(2)), Integer.parseInt(m.group(3)),
            Integer.parseInt(m.group(4)), Integer.parseInt(m.group(5)), Integer.parseInt(m.group(6)));
        LocalDateTime s = dt.minusMinutes(tzOffsetMinutes(tz));
        return canonicalYear(Integer.toString(s.getYear())) + "-" + pad2(s.getMonthValue()) + "-" + pad2(s.getDayOfMonth())
            + "T" + pad2(s.getHour()) + ":" + pad2(s.getMinute()) + ":" + pad2(s.getSecond()) + fraction + "Z";
    }

    public static String canonicalDate(String raw) {
        Matcher m = DATE.matcher(raw.trim());
        if (!m.matches())
            throw new IllegalArgumentException("canonicalDate: '" + raw + "' invalid");
        return canonicalYear(m.group(1)) + "-" + m.group(2) + "-" + m.group(3) + canonicalTz(m.group(4));
    }

    public static String canonicalTime(String raw) {
        Matcher m = TIME.matcher(raw.trim());
        if (!m.matches())
            throw new IllegalArgumentException("canonicalTime: '" + raw + "' invalid");
        String hh = m.group(1);
        String fraction = canonicalFraction(m.group(4));
        if (hh.equals("24") && m.group(2).equals("00") && m.group(3).equals("00") && fraction.isEmpty())
            hh = "00";
        return hh + ":" + m.group(2) + ":" + m.group(3) + fraction + canonicalTz(m.group(5));
    }

    public static String canonicalScalarLexical(Carrier c, String raw) {
        switch (c) {
            case INTEGER: return canonicalInteger(raw);
            case DECIMAL: return canonicalDecimal(raw);
            case DOUBLE: return canonicalDouble(raw);
            case FLOAT: return canonicalFloat(raw);
            case BOOLEAN: return canonicalBoolean(raw);
            case STRING: case ANY_URI: case LANG_STRING: return canonicalString(raw);
            case DATE_TIME: return canonicalDateTime(raw);
            case DATE: return canonicalDate(raw);
            case TIME: return canonicalTime(raw);
            case HEX_BINARY: return canonicalHexBinary(raw);
            case BASE64_BINARY: return canonicalBase64(raw);
            default: throw new IllegalArgumentException("unknown carrier " + c);
        }
    }

    // -- helpers ------------------------------------------------------------

    private static String canonicalYear(String raw) {
        boolean neg = raw.startsWith("-");
        String d = stripLeading(neg ? raw.substring(1) : raw, '0');
        if (d.isEmpty()) d = "0";
        while (d.length() < 4) d = "0" + d;
        return (neg ? "-" : "") + d;
    }

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

    private static int tzOffsetMinutes(String tz) {
        if (tz.equals("Z")) return 0;
        int sign = tz.charAt(0) == '-' ? -1 : 1;
        return sign * (Integer.parseInt(tz.substring(1, 3)) * 60 + Integer.parseInt(tz.substring(4, 6)));
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
