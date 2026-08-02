// Conformance: drive the Swift port with the shared golden wire vectors —
// op-scripts over a reader, and writer scripts asserting exact output bytes.
//
// Capability skips (never silent — the count is asserted and printed): Swift's
// writer params are exact-width types and a Swift `String` cannot hold an
// unpaired surrogate, so `wide-numeric-params`, `dynamic-numeric`, and
// `utf16-strings` cases are unrepresentable in this port. Same position as
// Rust/Go/C# on the numeric ones.

import Foundation
import XCTest
@testable import KanonakWire

/// Representability capabilities this port HAS. Empty: none of the three traps
/// the vectors gate on can be constructed in Swift.
private let capabilities: Set<String> = []

private func vectorsURL(_ name: String) -> URL {
    // .../kanonak-wire/swift/Tests/KanonakWireTests/ConformanceTests.swift
    URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()  // -> KanonakWireTests
        .deletingLastPathComponent()  // -> Tests
        .deletingLastPathComponent()  // -> swift
        .deletingLastPathComponent()  // -> kanonak-wire
        .appendingPathComponent("vectors")
        .appendingPathComponent(name)
}

private func hexToBytes(_ hex: String) -> [UInt8] {
    var out = [UInt8]()
    out.reserveCapacity(hex.count / 2)
    var i = hex.startIndex
    while i < hex.endIndex {
        let j = hex.index(i, offsetBy: 2)
        out.append(UInt8(hex[i..<j], radix: 16)!)
        i = j
    }
    return out
}

private func bytesToHex<C: Collection>(_ b: C) -> String where C.Element == UInt8 {
    b.map { String(format: "%02x", $0) }.joined()
}

/// What a read op yielded, in the shape the vector's `expected` compares against.
private enum ReadResult {
    case num(UInt64)
    case str(String)
    case hex(String)
    case unit
}

final class WireVectorTests: XCTestCase {

    private func checkError(_ id: String, _ op: String, _ error: Error, _ want: [String: Any]) {
        guard let e = error as? WireError else {
            XCTFail("[\(id)] \(op): expected a WireError, got \(error)")
            return
        }
        let wantKind = want["kind"] as? String ?? ""
        XCTAssertEqual(e.kind.rawValue, wantKind, "[\(id)] \(op): error kind")
        if let wantOffset = (want["offset"] as? NSNumber)?.intValue {
            XCTAssertEqual(e.offset, wantOffset, "[\(id)] \(op): error offset")
        }
    }

    private func runReadOp(_ r: inout WireReader, _ op: [String: Any]) throws -> ReadResult {
        let name = op["op"] as? String ?? ""
        let n = (op["n"] as? NSNumber)?.intValue ?? 0
        switch name {
        case "u8": return .num(UInt64(try r.u8()))
        case "u16be": return .num(UInt64(try r.u16BE()))
        case "u32be": return .num(UInt64(try r.u32BE()))
        case "bytes": return .hex(bytesToHex(try r.bytes(n)))
        case "uuid": return .str(try r.uuid())
        case "utf8": return .str(try r.utf8(n))
        case "lenPrefixedBytes16": return .hex(bytesToHex(try r.lenPrefixedBytes16()))
        case "rest": return .hex(bytesToHex(r.rest()))
        case "remaining": return .num(UInt64(r.remaining))
        case "expectEnd": try r.expectEnd(); return .unit
        default:
            XCTFail("conformance: unknown read op '\(name)'")
            return .unit
        }
    }

    private func runWriteOp(_ w: inout WireWriter, _ op: [String: Any]) throws {
        let name = op["op"] as? String ?? ""
        switch name {
        case "u8": w.u8(UInt8((op["value"] as! NSNumber).uint64Value))
        case "u16be": w.u16BE(UInt16((op["value"] as! NSNumber).uint64Value))
        case "u32be": w.u32BE(UInt32((op["value"] as! NSNumber).uint64Value))
        case "bytes": w.bytes(hexToBytes(op["hex"] as! String))
        case "uuid": try w.uuid(op["value"] as! String)
        case "utf8": w.utf8(op["value"] as! String)
        case "lenPrefixedBytes16": try w.lenPrefixedBytes16(hexToBytes(op["hex"] as! String))
        default: XCTFail("conformance: unknown write op '\(name)'")
        }
    }

    func testWireVectors() throws {
        let data = try Data(contentsOf: vectorsURL("wire-vectors.json"))
        let doc = try JSONSerialization.jsonObject(with: data) as! [String: Any]
        XCTAssertEqual(doc["wireFormatVersion"] as? String, wireFormatVersion)

        let readVectors = doc["readVectors"] as! [[String: Any]]
        let writeVectors = doc["writeVectors"] as! [[String: Any]]
        XCTAssertFalse(readVectors.isEmpty)
        XCTAssertFalse(writeVectors.isEmpty)

        var ran = 0
        var skipped = 0

        for v in readVectors {
            let id = v["id"] as! String
            if let req = v["requires"] as? String, !capabilities.contains(req) {
                skipped += 1
                continue
            }
            var r = WireReader(hexToBytes(v["bytes"] as! String))
            for op in v["ops"] as! [[String: Any]] {
                let opName = op["op"] as! String
                if let want = op["expectError"] as? [String: Any] {
                    do {
                        _ = try runReadOp(&r, op)
                        XCTFail("[\(id)] \(opName): expected \(want["kind"] ?? "an error"), got a value")
                    } catch {
                        checkError(id, opName, error, want)
                    }
                    break
                }
                let got = try runReadOp(&r, op)
                guard let expected = op["expected"] else { continue }
                switch got {
                case .num(let value):
                    XCTAssertEqual(value, (expected as! NSNumber).uint64Value, "[\(id)] \(opName)")
                case .str(let s), .hex(let s):
                    XCTAssertEqual(s, expected as! String, "[\(id)] \(opName)")
                case .unit:
                    break
                }
            }
            ran += 1
        }

        for v in writeVectors {
            let id = v["id"] as! String
            if let req = v["requires"] as? String, !capabilities.contains(req) {
                skipped += 1
                continue
            }
            var w = WireWriter()
            var errored = false
            for op in v["ops"] as! [[String: Any]] {
                let opName = op["op"] as! String
                if let want = op["expectError"] as? [String: Any] {
                    do {
                        try runWriteOp(&w, op)
                        XCTFail("[\(id)] \(opName): expected \(want["kind"] ?? "an error"), got success")
                    } catch {
                        checkError(id, opName, error, want)
                    }
                    errored = true
                    break
                }
                try runWriteOp(&w, op)
            }
            if !errored, let expected = v["expectedBytes"] as? String {
                XCTAssertEqual(bytesToHex(w.toBytes()), expected, "[\(id)] bytes")
            }
            ran += 1
        }

        // No silent coverage loss: every vector is either run or explicitly skipped.
        XCTAssertEqual(ran + skipped, readVectors.count + writeVectors.count)
        XCTAssertEqual(skipped, 6, "expected exactly the 6 capability-gated write vectors to skip")
        print("wire-vectors: \(ran)/\(readVectors.count + writeVectors.count) run (\(skipped) skipped)")
    }
}
