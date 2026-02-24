#!/usr/bin/env tsx
/**
 * Script to generate RPC method reference data from openrpc.json files
 * This reads the OpenRPC specifications for all API versions and creates structured JSON
 * that can be imported by the React component for display.
 */

import * as fs from "fs";
import * as path from "path";

interface OpenRPCParam {
  name: string;
  required: boolean;
  schema: any;
  description?: string;
}

interface OpenRPCResult {
  name: string;
  required: boolean;
  schema: any;
  description?: string;
}

interface OpenRPCMethod {
  name: string;
  description?: string;
  params: OpenRPCParam[];
  result: OpenRPCResult;
  paramStructure: string;
}

interface OpenRPCSchema {
  openrpc: string;
  info: {
    title: string;
    version: string;
  };
  methods: OpenRPCMethod[];
  components?: {
    schemas?: Record<string, any>;
  };
}

interface GeneratedMethodParam {
  name: string;
  required: boolean;
  type: string;
  description?: string;
  schemaRef?: string;
}

interface GeneratedMethod {
  name: string;
  namespace: string;
  shortName: string;
  description?: string;
  params: GeneratedMethodParam[];
  returnType: string;
  returnSchemaRef?: string;
  returnDescription?: string;
}

interface ApiVersion {
  version: string;
  methodCount: number;
  namespaces: string[];
  methods: GeneratedMethod[];
}

interface GeneratedReference {
  generatedAt: string;
  versions: ApiVersion[];
  schemas: Record<string, Record<string, any>>; // version -> schema name -> schema
}

/**
 * Resolve a JSON schema reference to extract the schema name
 */
function extractSchemaRef(schema: any): string | undefined {
  if (schema?.$ref) {
    return schema.$ref.split("/").pop();
  }
  return undefined;
}

/**
 * Resolve a JSON schema to a readable type string
 */
function resolveSchemaType(schema: any): string {
  if (!schema) {
    return "unknown";
  }

  // Handle $ref
  if (schema.$ref) {
    const refName = schema.$ref.split("/").pop();
    return refName || "unknown";
  }

  // Handle anyOf
  if (schema.anyOf) {
    const types = schema.anyOf.map((s: any) => resolveSchemaType(s));
    return types.join(" | ");
  }

  // Handle oneOf
  if (schema.oneOf) {
    const types = schema.oneOf.map((s: any) => resolveSchemaType(s));
    return types.join(" | ");
  }

  // Handle arrays with type union (e.g., ["array", "null"])
  if (Array.isArray(schema.type)) {
    if (schema.type.includes("array") && schema.items) {
      const itemType = resolveSchemaType(schema.items);
      const baseType = `Array<${itemType}>`;
      return schema.type.includes("null") ? `${baseType} | null` : baseType;
    }
    return (
      schema.type.filter((t: string) => t !== "null").join(" | ") +
      (schema.type.includes("null") ? " | null" : "")
    );
  }

  // Handle array
  if (schema.type === "array" && schema.items) {
    const itemType = resolveSchemaType(schema.items);
    return `Array<${itemType}>`;
  }

  // Handle object
  if (schema.type === "object") {
    return "object";
  }

  // Handle primitives
  if (schema.type) {
    let baseType = schema.type;
    if (schema.format) {
      baseType = `${schema.type} (${schema.format})`;
    }
    return baseType;
  }

  // Handle boolean schema (true means any type)
  if (schema === true) {
    return "any";
  }

  return "unknown";
}

/**
 * Process a single OpenRPC spec file
 */
function processVersion(
  versionName: string,
  specPath: string,
): { version: ApiVersion; schemas: Record<string, any> } {
  console.log(`Processing ${versionName}...`);
  const spec: OpenRPCSchema = JSON.parse(fs.readFileSync(specPath, "utf-8"));

  const components = spec.components?.schemas || {};
  const methods: GeneratedMethod[] = [];
  const namespaces = new Set<string>();

  for (const method of spec.methods) {
    // Handle both dot-notation (Filecoin.ChainHead) and underscore-notation (eth_blockNumber)
    let namespace: string;
    let shortName: string;

    if (method.name.includes(".")) {
      // Dot notation: "Filecoin.ChainHead" -> namespace: "Filecoin", shortName: "ChainHead"
      const parts = method.name.split(".");
      namespace = parts[0];
      shortName = parts.slice(1).join(".");
    } else if (method.name.includes("_")) {
      // Underscore notation: "eth_blockNumber" -> namespace: "eth", shortName: "blockNumber"
      const parts = method.name.split("_");
      namespace = parts[0];
      shortName = parts.slice(1).join("_");
    } else {
      // No separator: treat whole name as both namespace and shortName
      namespace = method.name;
      shortName = method.name;
    }

    namespaces.add(namespace);

    const params: GeneratedMethodParam[] = method.params.map((param) => ({
      name: param.name,
      required: param.required,
      type: resolveSchemaType(param.schema),
      description: param.description,
      schemaRef: extractSchemaRef(param.schema),
    }));

    const returnType = resolveSchemaType(method.result.schema);
    const returnSchemaRef = extractSchemaRef(method.result.schema);

    methods.push({
      name: method.name,
      namespace,
      shortName,
      description: method.description,
      params,
      returnType,
      returnSchemaRef,
      returnDescription: method.result.description,
    });
  }

  const version: ApiVersion = {
    version: versionName,
    methodCount: methods.length,
    namespaces: Array.from(namespaces).sort(),
    methods: methods.sort((a, b) => a.name.localeCompare(b.name)),
  };

  return { version, schemas: components };
}

/**
 * Parse all OpenRPC specs and generate method reference data
 */
function generateReference(openrpcDir: string, outputPath: string): void {
  console.log("Generating RPC reference from OpenRPC specifications...");

  const versions: ApiVersion[] = [];
  const allSchemas: Record<string, Record<string, any>> = {};

  // Process each version
  const versionFiles = ["v0.json", "v1.json", "v2.json"];

  for (const file of versionFiles) {
    const versionName = path.basename(file, ".json");
    const specPath = path.join(openrpcDir, file);

    if (!fs.existsSync(specPath)) {
      console.warn(`⚠ Skipping ${versionName} (file not found: ${specPath})`);
      continue;
    }

    const { version, schemas } = processVersion(versionName, specPath);
    versions.push(version);
    allSchemas[versionName] = schemas;

    console.log(
      `  ✓ ${versionName}: ${version.methodCount} methods, ${version.namespaces.length} namespaces`,
    );
  }

  const reference: GeneratedReference = {
    generatedAt: new Date().toISOString(),
    versions,
    schemas: allSchemas,
  };

  console.log("\nWriting generated reference data...");
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, JSON.stringify(reference, null, 2));

  const totalMethods = versions.reduce((sum, v) => sum + v.methodCount, 0);
  console.log(
    `\n✓ Generated reference for ${totalMethods} total methods across ${versions.length} API versions`,
  );
  console.log(`✓ Output written to: ${outputPath}`);
}

// Main execution
const OPENRPC_DIR = path.join(__dirname, "../openrpc-specs");
const OUTPUT_PATH = path.join(__dirname, "../src/data/rpc-methods.json");

try {
  generateReference(OPENRPC_DIR, OUTPUT_PATH);
} catch (error) {
  console.error("Error generating reference:", error);
  process.exit(1);
}
