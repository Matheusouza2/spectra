package racon;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** Dependency-free minimal JSON reader for the RACoN HTTP contract. */
final class Json {
    private final String text;
    private int index;

    private Json(String text) {
        this.text = text;
    }

    static Object parse(String text) {
        Json parser = new Json(text);
        parser.skipWhitespace();
        Object value = parser.readValue();
        parser.skipWhitespace();
        if (parser.index != text.length()) {
            throw parser.error("trailing content");
        }
        return value;
    }

    @SuppressWarnings("unchecked")
    static Map<String, Object> object(Object value) {
        if (!(value instanceof Map)) {
            throw new IllegalArgumentException("expected JSON object but found: " + value);
        }
        return (Map<String, Object>) value;
    }

    @SuppressWarnings("unchecked")
    static List<Object> array(Object value) {
        if (!(value instanceof List)) {
            throw new IllegalArgumentException("expected JSON array but found: " + value);
        }
        return (List<Object>) value;
    }

    static String string(Map<String, Object> object, String key) {
        Object value = object.get(key);
        return value == null ? null : value.toString();
    }

    private Object readValue() {
        if (index >= text.length()) {
            throw error("unexpected end of input");
        }
        char c = text.charAt(index);
        switch (c) {
            case '{':
                return readObject();
            case '[':
                return readArray();
            case '"':
                return readString();
            case 't':
                expectLiteral("true");
                return Boolean.TRUE;
            case 'f':
                expectLiteral("false");
                return Boolean.FALSE;
            case 'n':
                expectLiteral("null");
                return null;
            default:
                return readNumber();
        }
    }

    private Map<String, Object> readObject() {
        Map<String, Object> result = new LinkedHashMap<>();
        index++;
        skipWhitespace();
        if (peek() == '}') {
            index++;
            return result;
        }
        while (true) {
            skipWhitespace();
            String key = readString();
            skipWhitespace();
            expect(':');
            skipWhitespace();
            result.put(key, readValue());
            skipWhitespace();
            char c = text.charAt(index++);
            if (c == '}') {
                return result;
            }
            if (c != ',') {
                throw error("expected ',' or '}'");
            }
        }
    }

    private List<Object> readArray() {
        List<Object> result = new ArrayList<>();
        index++;
        skipWhitespace();
        if (peek() == ']') {
            index++;
            return result;
        }
        while (true) {
            skipWhitespace();
            result.add(readValue());
            skipWhitespace();
            char c = text.charAt(index++);
            if (c == ']') {
                return result;
            }
            if (c != ',') {
                throw error("expected ',' or ']'");
            }
        }
    }

    private String readString() {
        expect('"');
        StringBuilder builder = new StringBuilder();
        while (true) {
            if (index >= text.length()) {
                throw error("unterminated string");
            }
            char c = text.charAt(index++);
            if (c == '"') {
                return builder.toString();
            }
            if (c != '\\') {
                builder.append(c);
                continue;
            }
            char escape = text.charAt(index++);
            switch (escape) {
                case '"':
                    builder.append('"');
                    break;
                case '\\':
                    builder.append('\\');
                    break;
                case '/':
                    builder.append('/');
                    break;
                case 'b':
                    builder.append('\b');
                    break;
                case 'f':
                    builder.append('\f');
                    break;
                case 'n':
                    builder.append('\n');
                    break;
                case 'r':
                    builder.append('\r');
                    break;
                case 't':
                    builder.append('\t');
                    break;
                case 'u':
                    builder.append((char) Integer.parseInt(text.substring(index, index + 4), 16));
                    index += 4;
                    break;
                default:
                    throw error("invalid escape \\" + escape);
            }
        }
    }

    private Object readNumber() {
        int start = index;
        while (index < text.length() && "-+0123456789.eE".indexOf(text.charAt(index)) >= 0) {
            index++;
        }
        String token = text.substring(start, index);
        if (token.isEmpty()) {
            throw error("expected a value");
        }
        if (token.indexOf('.') < 0 && token.indexOf('e') < 0 && token.indexOf('E') < 0) {
            return Long.parseLong(token);
        }
        return Double.parseDouble(token);
    }

    private void expectLiteral(String literal) {
        if (!text.startsWith(literal, index)) {
            throw error("expected " + literal);
        }
        index += literal.length();
    }

    private void expect(char expected) {
        if (index >= text.length() || text.charAt(index) != expected) {
            throw error("expected '" + expected + "'");
        }
        index++;
    }

    private char peek() {
        if (index >= text.length()) {
            throw error("unexpected end of input");
        }
        return text.charAt(index);
    }

    private void skipWhitespace() {
        while (index < text.length() && Character.isWhitespace(text.charAt(index))) {
            index++;
        }
    }

    private IllegalArgumentException error(String message) {
        return new IllegalArgumentException("invalid JSON at offset " + index + ": " + message);
    }
}
