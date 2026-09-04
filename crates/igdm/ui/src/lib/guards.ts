// Runtime predicates for payloads decoded from the wire or passed through IPC.
// Checking `typeof` inline narrows a representation without establishing its
// contract; object payloads are decoded into a `JsonObject` at the I/O
// boundary, and scalar checks use a named predicate that gives TypeScript a
// real type predicate.

/** A JSON-encodable value. The recursive object member and array member give
 * keys a concrete value type instead of an `unknown` escape hatch. */
export type Json = string | number | boolean | null | Json[] | { [key: string]: Json };

/** A JSON object: a string-keyed record whose values are `Json`. */
export type JsonObject = { [key: string]: Json };

export function isString(value: unknown): value is string {
  return typeof value === "string";
}

export function isNumber(value: unknown): value is number {
  return typeof value === "number";
}
