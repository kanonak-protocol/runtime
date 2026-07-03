using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Text.Json;
using Kanonak.Wire;

// Conformance runner: drives the shared wire vectors through the C#
// kanonak-wire port. Read vectors run an op-script against a hex buffer
// asserting values or required errors {kind, offset}; write vectors run
// writer ops asserting exact output bytes. Exits non-zero on any failure.
//   dotnet run -- <vectors-dir>

class Program
{
    // C# strings are UTF-16 (can hold an unpaired surrogate). Writer numeric
    // parameters are exact-width (byte/ushort/uint) and static — no
    // wide-numeric-params, no dynamic-numeric.
    static readonly HashSet<string> Capabilities = new HashSet<string> { "utf16-strings" };

    static int _pass;
    static int _fail;
    static int _skipped;

    static int Main(string[] args)
    {
        string vectorsDir = args.Length > 0 ? args[0] : FindVectorsDir();
        if (vectorsDir == null || !Directory.Exists(vectorsDir))
        {
            Console.Error.WriteLine("vectors directory not found; pass it as the first argument");
            return 2;
        }

        using var doc = JsonDocument.Parse(File.ReadAllText(Path.Combine(vectorsDir, "wire-vectors.json")));
        var root = doc.RootElement;

        int total = 0;
        foreach (var v in root.GetProperty("readVectors").EnumerateArray())
        {
            total++;
            RunReadVector(v);
        }
        foreach (var v in root.GetProperty("writeVectors").EnumerateArray())
        {
            total++;
            RunWriteVector(v);
        }

        Console.WriteLine($"wire-vectors: {_pass}/{total} pass ({_skipped} skipped)");
        if (_fail > 0)
        {
            Console.Error.WriteLine($"{_fail} VECTOR(S) FAILED");
            return 1;
        }
        Console.WriteLine("ALL VECTORS PASS");
        return 0;
    }

    static bool IsSkipped(JsonElement v)
    {
        if (v.TryGetProperty("requires", out var req) && !Capabilities.Contains(req.GetString()))
        {
            _skipped++;
            return true;
        }
        return false;
    }

    static void RunReadVector(JsonElement v)
    {
        if (IsSkipped(v)) return;
        string id = v.GetProperty("id").GetString();
        var reader = new WireReader(HexToBytes(v.GetProperty("bytes").GetString()));
        bool ok = true;
        foreach (var op in v.GetProperty("ops").EnumerateArray())
        {
            string opName = op.GetProperty("op").GetString();
            if (op.TryGetProperty("expectError", out var expectError))
            {
                try
                {
                    RunReadOp(reader, opName, op);
                    FailCase(id, $"{opName}: expected {expectError.GetProperty("kind").GetString()}, got a value");
                    ok = false;
                }
                catch (Exception e)
                {
                    if (!CheckError(id, opName, e, expectError)) ok = false;
                }
                break; // an error op ends the script
            }
            object got;
            try
            {
                got = RunReadOp(reader, opName, op);
            }
            catch (Exception e)
            {
                FailCase(id, $"{opName}: threw {e.Message}");
                ok = false;
                break;
            }
            if (op.TryGetProperty("expected", out var expected) && !ValueMatches(got, expected))
            {
                FailCase(id, $"{opName}: expected {expected}, got {got}");
                ok = false;
                break;
            }
        }
        if (ok) _pass++;
    }

    static object RunReadOp(WireReader r, string opName, JsonElement op)
    {
        switch (opName)
        {
            case "u8": return (ulong)r.U8();
            case "u16be": return (ulong)r.U16Be();
            case "u32be": return (ulong)r.U32Be();
            case "bytes": return BytesToHex(r.Bytes(op.GetProperty("n").GetInt32()));
            case "uuid": return r.Uuid();
            case "utf8": return r.Utf8(op.GetProperty("n").GetInt32());
            case "lenPrefixedBytes16": return BytesToHex(r.LenPrefixedBytes16());
            case "rest": return BytesToHex(r.Rest());
            case "remaining": return (ulong)r.Remaining;
            case "expectEnd": r.ExpectEnd(); return null;
            default: throw new InvalidOperationException($"conformance: unknown read op '{opName}'");
        }
    }

    static void RunWriteVector(JsonElement v)
    {
        if (IsSkipped(v)) return;
        string id = v.GetProperty("id").GetString();
        var writer = new WireWriter();
        bool ok = true;
        foreach (var op in v.GetProperty("ops").EnumerateArray())
        {
            string opName = op.GetProperty("op").GetString();
            if (op.TryGetProperty("expectError", out var expectError))
            {
                try
                {
                    RunWriteOp(writer, opName, op);
                    FailCase(id, $"{opName}: expected {expectError.GetProperty("kind").GetString()}, got success");
                    ok = false;
                }
                catch (Exception e)
                {
                    if (!CheckError(id, opName, e, expectError)) ok = false;
                }
                break;
            }
            try
            {
                RunWriteOp(writer, opName, op);
            }
            catch (Exception e)
            {
                FailCase(id, $"{opName}: threw {e.Message}");
                ok = false;
                break;
            }
        }
        if (ok && v.TryGetProperty("expectedBytes", out var expectedBytes))
        {
            string got = BytesToHex(writer.ToBytes());
            if (got != expectedBytes.GetString())
            {
                FailCase(id, $"expected bytes {expectedBytes.GetString()}, got {got}");
                ok = false;
            }
        }
        if (ok) _pass++;
    }

    static void RunWriteOp(WireWriter w, string opName, JsonElement op)
    {
        switch (opName)
        {
            case "u8": w.U8(op.GetProperty("value").GetByte()); return;
            case "u16be": w.U16Be(op.GetProperty("value").GetUInt16()); return;
            case "u32be": w.U32Be(op.GetProperty("value").GetUInt32()); return;
            case "bytes": w.Bytes(HexToBytes(op.GetProperty("hex").GetString())); return;
            case "uuid": w.Uuid(op.GetProperty("value").GetString()); return;
            case "utf8": w.Utf8(WriteOpString(op)); return;
            case "lenPrefixedBytes16": w.LenPrefixedBytes16(HexToBytes(op.GetProperty("hex").GetString())); return;
            default: throw new InvalidOperationException($"conformance: unknown write op '{opName}'");
        }
    }

    static string WriteOpString(JsonElement op)
    {
        if (op.TryGetProperty("utf16CodeUnits", out var units))
        {
            var chars = new List<char>();
            foreach (var u in units.EnumerateArray()) chars.Add((char)u.GetUInt16());
            return new string(chars.ToArray());
        }
        return op.GetProperty("value").GetString();
    }

    static bool CheckError(string id, string opName, Exception e, JsonElement want)
    {
        if (!(e is WireError we))
        {
            FailCase(id, $"{opName}: threw a non-WireError: {e.Message}");
            return false;
        }
        string wantKind = want.GetProperty("kind").GetString();
        if (we.Kind != wantKind)
        {
            FailCase(id, $"{opName}: expected error kind {wantKind}, got {we.Kind} ({we.Message})");
            return false;
        }
        if (want.TryGetProperty("offset", out var off) && we.Offset != off.GetInt32())
        {
            string gotOffset = we.Offset.HasValue ? we.Offset.Value.ToString() : "null";
            FailCase(id, $"{opName}: expected error offset {off.GetInt32()}, got {gotOffset}");
            return false;
        }
        return true;
    }

    static bool ValueMatches(object got, JsonElement expected)
    {
        if (expected.ValueKind == JsonValueKind.String) return (got as string) == expected.GetString();
        if (expected.ValueKind == JsonValueKind.Number) return got is ulong u && u == expected.GetUInt64();
        return false;
    }

    static void FailCase(string id, string message)
    {
        _fail++;
        Console.Error.WriteLine($"{id}: {message}");
    }

    static byte[] HexToBytes(string hex)
    {
        var outBytes = new byte[hex.Length / 2];
        for (int i = 0; i < outBytes.Length; i++)
        {
            outBytes[i] = Convert.ToByte(hex.Substring(i * 2, 2), 16);
        }
        return outBytes;
    }

    static string BytesToHex(ReadOnlyMemory<byte> b)
    {
        var sb = new StringBuilder(b.Length * 2);
        foreach (byte x in b.Span)
        {
            sb.Append(x.ToString("x2"));
        }
        return sb.ToString();
    }

    static string BytesToHex(byte[] b)
    {
        return BytesToHex(new ReadOnlyMemory<byte>(b));
    }

    static string FindVectorsDir()
    {
        var dir = new DirectoryInfo(Directory.GetCurrentDirectory());
        while (dir != null)
        {
            string candidate = Path.Combine(dir.FullName, "vectors");
            if (File.Exists(Path.Combine(candidate, "wire-vectors.json"))) return candidate;
            candidate = Path.Combine(dir.FullName, "kanonak-wire", "vectors");
            if (File.Exists(Path.Combine(candidate, "wire-vectors.json"))) return candidate;
            dir = dir.Parent;
        }
        return null;
    }
}
