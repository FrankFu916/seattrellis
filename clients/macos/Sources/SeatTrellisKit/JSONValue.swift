import Foundation

/// An editable projection must never become the persisted source. This value
/// retains unknown fields, optional attributes and full-width integer seeds.
public enum JSONValue: Codable, Equatable, Sendable {
    case object([String: JSONValue]), array([JSONValue]), string(String)
    case integer(Int64), unsigned(UInt64), number(Double), bool(Bool), null

    public init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer()
        if value.decodeNil() { self = .null }
        else if let item = try? value.decode(Bool.self) { self = .bool(item) }
        else if let item = try? value.decode(Int64.self) { self = .integer(item) }
        else if let item = try? value.decode(UInt64.self) { self = .unsigned(item) }
        else if let item = try? value.decode(Double.self) { self = .number(item) }
        else if let item = try? value.decode(String.self) { self = .string(item) }
        else if let item = try? value.decode([JSONValue].self) { self = .array(item) }
        else { self = .object(try value.decode([String: JSONValue].self)) }
    }

    public func encode(to encoder: Encoder) throws {
        var value = encoder.singleValueContainer()
        switch self {
        case .object(let item): try value.encode(item)
        case .array(let item): try value.encode(item)
        case .string(let item): try value.encode(item)
        case .integer(let item): try value.encode(item)
        case .unsigned(let item): try value.encode(item)
        case .number(let item): try value.encode(item)
        case .bool(let item): try value.encode(item)
        case .null: try value.encodeNil()
        }
    }

    public subscript(_ key: String) -> JSONValue {
        get { if case .object(let value) = self { return value[key] ?? .null }; return .null }
        set { if case .object(var value) = self { value[key] = newValue; self = .object(value) } }
    }
    public var string: String? { if case .string(let item) = self { return item }; return nil }
    public var array: [JSONValue]? { if case .array(let item) = self { return item }; return nil }
    public var object: [String: JSONValue]? { if case .object(let item) = self { return item }; return nil }
    public var bool: Bool? { if case .bool(let item) = self { return item }; return nil }
    public var display: String {
        switch self {
        case .string(let item): return item
        case .integer(let item): return String(item)
        case .unsigned(let item): return String(item)
        case .number(let item): return String(item)
        default: return "—"
        }
    }
    public func data(pretty: Bool = false) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = pretty ? [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes] : [.sortedKeys]
        return try encoder.encode(self)
    }
    public func text() throws -> String { String(decoding: try data(pretty: true), as: UTF8.self) }
    public func decode<T: Decodable>(_ type: T.Type) throws -> T { try JSONDecoder().decode(type, from: data()) }
    public static func parse(_ data: Data) throws -> JSONValue { try JSONDecoder().decode(Self.self, from: data) }
}
