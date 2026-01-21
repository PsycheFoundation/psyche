/// Most of this file is copy-pasted from Anchor,
// but I had to change some stuff to make it type infer correctly.

import { Buffer } from "buffer";
import { PsycheSolanaCoordinator, PsycheSolanaMiningPool } from "shared";
import BN from "bn.js";
import { PublicKey } from "@solana/web3.js";
import { IdlAccounts } from "@coral-xyz/anchor";

type Idl = {
  address: string;
  metadata: IdlMetadata;
  docs?: string[];
  instructions: IdlInstruction[];
  accounts?: IdlAccount[];
  events?: IdlEvent[];
  errors?: IdlErrorCode[];
  types?: IdlTypeDef[];
  constants?: IdlConst[];
};

type IdlMetadata = {
  name: string;
  version: string;
  spec: string;
  description?: string;
  repository?: string;
  dependencies?: IdlDependency[];
  contact?: string;
  deployments?: IdlDeployments;
};

type IdlDependency = {
  name: string;
  version: string;
};

type IdlDeployments = {
  mainnet?: string;
  testnet?: string;
  devnet?: string;
  localnet?: string;
};

type IdlInstruction = {
  name: string;
  docs?: string[];
  discriminator: IdlDiscriminator;
  accounts: IdlInstructionAccountItem[];
  args: IdlField[];
  returns?: IdlType;
};

type IdlInstructionAccountItem = IdlInstructionAccount | IdlInstructionAccounts;

type IdlInstructionAccount = {
  name: string;
  docs?: string[];
  writable?: boolean;
  signer?: boolean;
  optional?: boolean;
  address?: string;
  pda?: IdlPda;
  relations?: string[];
};

type IdlInstructionAccounts = {
  name: string;
  accounts: IdlInstructionAccount[];
};

type IdlPda = {
  seeds: IdlSeed[];
  program?: IdlSeed;
};

type IdlSeed = IdlSeedConst | IdlSeedArg | IdlSeedAccount;

type IdlSeedConst = {
  kind: "const";
  value: number[];
};

type IdlSeedArg = {
  kind: "arg";
  path: string;
};

type IdlSeedAccount = {
  kind: "account";
  path: string;
  account?: string;
};

type IdlAccount = {
  name: string;
  discriminator: IdlDiscriminator;
};

type IdlEvent = {
  name: string;
  discriminator: IdlDiscriminator;
};

type IdlConst = {
  name: string;
  type: IdlType;
  value: string;
};

type IdlErrorCode = {
  name: string;
  code: number;
  msg?: string;
};

type IdlField = {
  name: string;
  docs?: string[];
  type: IdlType;
};

type IdlTypeDef = {
  name: string;
  docs?: string[];
  serialization?: IdlSerialization;
  repr?: IdlRepr;
  generics?: IdlTypeDefGeneric[];
  type: IdlTypeDefTy;
};

type IdlSerialization =
  | "borsh"
  | "bytemuck"
  | "bytemuckunsafe"
  | { custom: string };

type IdlRepr = IdlReprRust | IdlReprC | IdlReprTransparent;

type IdlReprRust = {
  kind: "rust";
} & IdlReprModifier;

type IdlReprC = {
  kind: "c";
} & IdlReprModifier;

type IdlReprTransparent = {
  kind: "transparent";
};

type IdlReprModifier = {
  packed?: boolean;
  align?: number;
};

type IdlTypeDefGeneric = IdlTypeDefGenericType | IdlTypeDefGenericConst;

type IdlTypeDefGenericType = {
  kind: "type";
  name: string;
};

type IdlTypeDefGenericConst = {
  kind: "const";
  name: string;
  type: string;
};

type IdlTypeDefTy = IdlTypeDefTyEnum | IdlTypeDefTyStruct | IdlTypeDefTyType;

type IdlTypeDefTyStruct = {
  kind: "struct";
  fields?: IdlDefinedFields;
};

type IdlTypeDefTyEnum = {
  kind: "enum";
  variants: IdlEnumVariant[];
};

type IdlTypeDefTyType = {
  kind: "type";
  alias: IdlType;
};

type IdlEnumVariant = {
  name: string;
  fields?: IdlDefinedFields;
};

type IdlDefinedFields = IdlDefinedFieldsNamed | IdlDefinedFieldsTuple;

type IdlDefinedFieldsNamed = IdlField[];

type IdlDefinedFieldsTuple = IdlType[];

type IdlArrayLen = IdlArrayLenGeneric | IdlArrayLenValue;

type IdlArrayLenGeneric = {
  generic: string;
};

type IdlArrayLenValue = number;

type IdlGenericArg = IdlGenericArgType | IdlGenericArgConst;

type IdlGenericArgType = { kind: "type"; type: IdlType };

type IdlGenericArgConst = { kind: "const"; value: string };

type IdlType =
  | "bool"
  | "u8"
  | "i8"
  | "u16"
  | "i16"
  | "u32"
  | "i32"
  | "f32"
  | "u64"
  | "i64"
  | "f64"
  | "u128"
  | "i128"
  | "u256"
  | "i256"
  | "bytes"
  | "string"
  | "pubkey"
  | IdlTypeOption
  | IdlTypeCOption
  | IdlTypeVec
  | IdlTypeArray
  | IdlTypeDefined
  | IdlTypeGeneric;

type IdlTypeOption = {
  option: IdlType;
};

type IdlTypeCOption = {
  coption: IdlType;
};

type IdlTypeVec = {
  vec: IdlType;
};

type IdlTypeArray = {
  array: [idlType: IdlType, size: IdlArrayLen];
};

type IdlTypeDefined = {
  defined: {
    name: string;
    generics?: IdlGenericArg[];
  };
};

type IdlTypeGeneric = {
  generic: string;
};

type IdlDiscriminator = number[];

type Address = PublicKey | string;

/**
 * A set of accounts mapping one-to-one to the program's accounts struct, i.e.,
 * the type deriving `#[derive(Accounts)]`.
 *
 * The name of each field should match the name for that account in the IDL.
 *
 * If multiple accounts are nested in the rust program, then they should be
 * nested here.
 */
type Accounts<A extends IdlInstructionAccountItem = IdlInstructionAccountItem> =
  {
    [N in A["name"]]: Account<A & { name: N }>;
  };

type Account<A extends IdlInstructionAccountItem> =
  A extends IdlInstructionAccounts
    ? Accounts<A["accounts"][number]>
    : A extends { optional: true }
      ? Address | null
      : A extends { signer: true }
        ? Address | undefined
        : Address;

/**
 * All instructions for an IDL.
 */
type AllInstructions<I extends Idl> = I["instructions"][number];

type TypeMap = {
  pubkey: PublicKey;
  bool: boolean;
  string: string;
  bytes: Buffer;
} & {
  [K in "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "f32" | "f64"]: number;
} & {
  [K in "u64" | "i64" | "u128" | "i128" | "u256" | "i256"]: BN;
};

type DecodeType<T extends IdlType, Defined> = IdlType extends T
  ? unknown
  : T extends keyof TypeMap
    ? TypeMap[T]
    : T extends { defined: { name: keyof Defined } }
      ? Defined[T["defined"]["name"]]
      : T extends { option: IdlType }
        ? DecodeType<T["option"], Defined> | null
        : T extends { coption: IdlType }
          ? DecodeType<T["coption"], Defined> | null
          : T extends { vec: IdlType }
            ? DecodeType<T["vec"], Defined>[]
            : T extends {
                  array: [defined: IdlType, size: IdlArrayLen];
                }
              ? DecodeType<T["array"][0], Defined>[]
              : unknown;

type ArgsTuple<A extends IdlField[], Defined> = {
  [K in keyof A]: A[K] extends IdlField
    ? DecodeType<A[K]["type"], Defined>
    : unknown;
} & unknown[];

type UnboxToUnion<T> = T extends (infer U)[]
  ? UnboxToUnion<U>
  : T extends Record<string, never> // empty object, eg: named enum variant without fields
    ? "__empty_object__"
    : T extends Record<string, infer V> // object with props, eg: struct
      ? UnboxToUnion<V>
      : T;

type DecodeDefinedField<F, Defined> = F extends IdlType
  ? DecodeType<F, Defined>
  : never;

/**
 * decode enum variant: named or tuple
 */
type DecodeDefinedFields<
  F extends IdlDefinedFields,
  Defined,
> = F extends IdlDefinedFieldsNamed
  ? {
      [F2 in F[number] as F2["name"]]: DecodeDefinedField<F2["type"], Defined>;
    }
  : F extends IdlDefinedFieldsTuple
    ? {
        [F3 in keyof F as Exclude<F3, keyof unknown[]>]: DecodeDefinedField<
          F[F3],
          Defined
        >;
      }
    : Record<string, never>;

type DecodeEnumVariants<I extends IdlTypeDefTyEnum, Defined> = {
  [V in I["variants"][number] as V["name"]]: DecodeDefinedFields<
    NonNullable<V["fields"]>,
    Defined
  >;
};

type ValueOf<T> = T[keyof T];
type XorEnumVariants<T extends Record<string, unknown>> = ValueOf<{
  [K1 in keyof T]: {
    [K2 in Exclude<keyof T, K1>]?: never;
  } & { [K2 in K1]: T[K2] };
}>;

type DecodeEnum<I extends IdlTypeDefTyEnum, Defined> = XorEnumVariants<
  DecodeEnumVariants<I, Defined>
>;

type DecodeStruct<I extends IdlTypeDefTyStruct, Defined> = DecodeDefinedFields<
  NonNullable<I["fields"]>,
  Defined
>;

type DecodeAlias<I extends IdlTypeDefTyType, Defined> = DecodeType<
  I["alias"],
  Defined
>;

type TypeDef<I extends IdlTypeDef, Defined> = I["type"] extends IdlTypeDefTyEnum
  ? DecodeEnum<I["type"], Defined>
  : I["type"] extends IdlTypeDefTyStruct
    ? DecodeStruct<I["type"], Defined>
    : I["type"] extends IdlTypeDefTyType
      ? DecodeAlias<I["type"], Defined>
      : never;

type DecodedHelper<T extends IdlTypeDef[], Defined> = {
  [D in T[number] as D["name"]]: TypeDef<D, Defined>;
};

type UnknownType = "__unknown_defined_type__";
/**
 * Empty "defined" object to produce `UnknownType` instead of never/unknown
 * during IDL types decoding.
 */
type EmptyDefined = Record<UnknownType, never>;

type RecursiveDepth2<
  T extends IdlTypeDef[],
  Defined = EmptyDefined,
  Decoded = DecodedHelper<T, Defined>,
> =
  UnknownType extends UnboxToUnion<Decoded>
    ? RecursiveDepth3<T, DecodedHelper<T, Defined>>
    : Decoded;

type RecursiveDepth3<
  T extends IdlTypeDef[],
  Defined = EmptyDefined,
  Decoded = DecodedHelper<T, Defined>,
> =
  UnknownType extends UnboxToUnion<Decoded>
    ? RecursiveDepth4<T, DecodedHelper<T, Defined>>
    : Decoded;

type RecursiveDepth4<
  T extends IdlTypeDef[],
  Defined = EmptyDefined,
> = DecodedHelper<T, Defined>;

/**
 * TypeScript can't handle truly recursive type (RecursiveTypes instead of RecursiveDepth2).
 * Hence we're doing recursion of depth=4 manually
 *  */
type RecursiveTypes<
  T extends IdlTypeDef[],
  Defined = EmptyDefined,
  Decoded = DecodedHelper<T, Defined>,
> =
  // check if some of decoded types is Unknown (not decoded properly)
  UnknownType extends UnboxToUnion<Decoded>
    ? RecursiveDepth2<T, DecodedHelper<T, Defined>>
    : Decoded;

type IdlTypes<I extends Idl> = RecursiveTypes<NonNullable<I["types"]>>;

type InstructionDataUnion<I extends Idl> = {
  [K in AllInstructions<I>["name"]]: {
    name: K;
    data: ArgsTuple<
      Extract<AllInstructions<I>, { name: K }>["args"],
      IdlTypes<I>
    > extends [...infer Args]
      ? Args["length"] extends 0
        ? Record<string, never>
        : {
            [Key in Extract<
              AllInstructions<I>,
              { name: K }
            >["args"][number]["name"]]: DecodeType<
              Extract<
                Extract<AllInstructions<I>, { name: K }>["args"][number],
                { name: Key }
              >["type"],
              IdlTypes<I>
            >;
          }
      : never;
  };
}[AllInstructions<I>["name"]];

type ToSnakeCase<S extends string> = S extends `${infer T}${infer U}`
  ? T extends Uppercase<T>
    ? T extends `${number}`
      ? `${T}${ToSnakeCase<U>}` // if it's a number, don't add an underscore
      : `_${Lowercase<T>}${ToSnakeCase<U>}` // else it's an uppercase letter, so add underscore
    : `${T}${ToSnakeCase<U>}`
  : S;

type RemoveLeadingUnderscore<S extends string> = S extends `_${infer R}`
  ? R
  : S;

type ToSnakeCaseObject<T> = T extends object
  ? T extends Array<infer U>
    ? Array<ToSnakeCaseObject<U>>
    : T extends Date
      ? T
      : T extends PublicKey
        ? T
        : T extends BN
          ? T
          : T extends Set<infer U>
            ? Set<ToSnakeCaseObject<U>>
            : T extends Map<infer K, infer V>
              ? Map<ToSnakeCaseObject<K>, ToSnakeCaseObject<V>>
              : {
                  [K in keyof T as RemoveLeadingUnderscore<
                    ToSnakeCase<K & string>
                  >]: T[K] extends string
                    ? ToSnakeCase<T[K]>
                    : ToSnakeCaseObject<T[K]>;
                }
  : T;

export type PsycheCoordinatorInstructionsUnion = ToSnakeCaseObject<
  InstructionDataUnion<PsycheSolanaCoordinator>
>;

type Extends<T, U> = T extends U ? T : never;

export type WitnessMetadata = Extends<
  PsycheCoordinatorInstructionsUnion,
  { name: "witness" }
>["data"]["metadata"];
export type WitnessEvalResult =
  IdlTypes<PsycheSolanaCoordinator>["witnessEvalResult"];

export type PsycheMiningPoolInstructionsUnion = ToSnakeCaseObject<
  InstructionDataUnion<PsycheSolanaMiningPool>
>;

export type PsycheMiningPoolAccount =
  IdlAccounts<PsycheSolanaMiningPool>["pool"];
export type PsycheMiningPoolLenderAccount =
  IdlAccounts<PsycheSolanaMiningPool>["lender"];
