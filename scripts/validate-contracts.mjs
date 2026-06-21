import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function resolveRef(schemaRoot, ref) {
  if (!ref.startsWith("#/")) throw new Error(`unsupported external schema reference: ${ref}`);
  return ref.slice(2).split("/").reduce((value, key) => value[key], schemaRoot);
}

function matchesType(value, type) {
  if (type === "null") return value === null;
  if (type === "array") return Array.isArray(value);
  if (type === "object") return value !== null && typeof value === "object" && !Array.isArray(value);
  if (type === "integer") return Number.isInteger(value);
  return typeof value === type;
}

function validate(value, schema, schemaRoot, location = "$") {
  if (schema.$ref) return validate(value, resolveRef(schemaRoot, schema.$ref), schemaRoot, location);
  if (schema.const !== undefined && value !== schema.const) throw new Error(`${location}: expected constant ${schema.const}`);
  if (schema.enum && !schema.enum.includes(value)) throw new Error(`${location}: value is not in enum`);
  if (schema.type) {
    const types = Array.isArray(schema.type) ? schema.type : [schema.type];
    if (!types.some((type) => matchesType(value, type))) throw new Error(`${location}: expected ${types.join(" or ")}`);
  }
  if (typeof value === "string") {
    if (schema.minLength !== undefined && value.length < schema.minLength) throw new Error(`${location}: string is too short`);
    if (schema.pattern && !new RegExp(schema.pattern).test(value)) throw new Error(`${location}: string does not match ${schema.pattern}`);
    if (schema.format === "uri") {
      let parsed;
      try { parsed = new URL(value); } catch { throw new Error(`${location}: invalid URI`); }
      if (!parsed.protocol || !parsed.hostname) throw new Error(`${location}: invalid URI`);
    }
    if (schema.format === "date" && !/^\d{4}-\d{2}-\d{2}$/.test(value)) throw new Error(`${location}: invalid date`);
  }
  if (Array.isArray(value)) {
    if (schema.minItems !== undefined && value.length < schema.minItems) throw new Error(`${location}: array is too short`);
    if (schema.uniqueItems && new Set(value.map((item) => JSON.stringify(item))).size !== value.length) throw new Error(`${location}: duplicate array items`);
    if (schema.items) value.forEach((item, index) => validate(item, schema.items, schemaRoot, `${location}[${index}]`));
  }
  if (value !== null && typeof value === "object" && !Array.isArray(value)) {
    for (const key of schema.required ?? []) if (!(key in value)) throw new Error(`${location}: missing required property ${key}`);
    const properties = schema.properties ?? {};
    if (schema.additionalProperties === false) {
      for (const key of Object.keys(value)) if (!(key in properties)) throw new Error(`${location}: unexpected property ${key}`);
    }
    for (const [key, child] of Object.entries(value)) if (properties[key]) validate(child, properties[key], schemaRoot, `${location}.${key}`);
  }
}

function validateDirectory(directory, schemaFile) {
  const schema = readJson(path.join(root, "schemas", schemaFile));
  const target = path.join(root, directory);
  if (!fs.existsSync(target)) return 0;
  const files = fs.readdirSync(target).filter((file) => file.endsWith(".json")).sort();
  for (const file of files) validate(readJson(path.join(target, file)), schema, schema, `${directory}/${file}`);
  return files.length;
}

for (const file of fs.readdirSync(path.join(root, "schemas")).filter((file) => file.endsWith(".json"))) readJson(path.join(root, "schemas", file));
const apps = validateDirectory("manifests/apps", "app-manifest.schema.json");
const tools = validateDirectory("manifests/tools", "tool-manifest.schema.json");
console.log(`Validated ${apps} application manifests and ${tools} tool manifests against public schemas.`);
