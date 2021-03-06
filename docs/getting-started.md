# Getting Started
> **Note**:  this project is heavily WIP and until it reaches v1.0 the API is subject to change in breaking ways without notice.

## 1 Introduction

### 1.1 What is the Graph?

The Graph is a decentralized protocol for indexing and querying data from blockchains, starting with Ethereum. It makes it possible to query for data that is difficult or impossible to query for directly.

For example, with the popular Cryptokitties dApp which implements the [ERC-721 Non-Fungible Token (NFT)](https://github.com/ethereum/eips/issues/721) standard, it is relatively straight forward to ask the following questions:
> How many cryptokitties does a specific Ethereum account own?
> When was a particular cryptokitty born?

This is because these read patterns are directly supported by the methods exposed by the [contract](https://github.com/dapperlabs/cryptokitties-bounty/blob/master/contracts/KittyCore.sol): the [`balanceOf`](https://github.com/dapperlabs/cryptokitties-bounty/blob/master/contracts/KittyOwnership.sol#L64) and [`getKitty`](https://github.com/dapperlabs/cryptokitties-bounty/blob/master/contracts/KittyCore.sol#L91) methods, respectively.

However, other questions are more difficult to answer:
> Who are the owners of the cryptokitties born between January and February of 2018?

For this you would need to process all [`Birth` events](https://github.com/dapperlabs/cryptokitties-bounty/blob/master/contracts/KittyBase.sol#L15) and then call the [`ownerOf` method](https://github.com/dapperlabs/cryptokitties-bounty/blob/master/contracts/KittyOwnership.sol#L144) for each cryptokitty that has been born into existence. (An alternate approach could involve processing all [`Transfer` events] and filtering on the most recent transfer for each cryptokitty in existence).

Even for this relatively simple question, it would take hours to days for a decentralized application (dApp) running in a browser to get an answer. Indexing and caching data off blockchains is hard. There are edge cases around finality, chain reorganizations, uncled blocks, etc.

The Graph solves this today by providing an open source node implementation, [Graph Node](../README.md), which handles indexing and caching of blockchain data, which the entire community can contribute to and leverage. It exposes this functionality through a GraphQL API.

### 1.2 How does it work?

The Graph must be run alongside a running IPFS node, Ethereum node and a store (Postgres, in this initial implementation).

![Data Flow Diagram](images/TheGraph_DataFlowDiagram.png)

The high level data flow is as follows:
1. A decentralized application creates/modifies data on Ethereum through a transaction to a smart contract.
2. The smart contract emits one or more events (logs) while processing the transaction.
3. Graph Node listens for specific events and fires handlers in a user-defined mapping.
4. The mapping is a WASM module which runs in a WASM runtime. It creates one or more store transactions in response to Ethereum events.
5. The store is updated along with indexes.
6. A decentralized application queries--via a GraphQL endpoint--the Graph Node, which in turn queries the store, for data which was ingested from the blockchain. This may include complex queries which take advantage of the store's indexes.
7. The decentralized application displays this data in a rich UI, which an end-user leverages in making new transactions against the Ethereum blockchain.
8. The cycle repeats.

### 1.3 What's included?
There are two repos relevant to building on The Graph:
1. [Graph Node](../README.md) (this repo) - A server implementation for indexing, caching and serving queries against data from Ethereum.
1. [Graph CLI](https://github.com/graphprotocol/graph-cli) - A CLI for building and compiling projects which are deployed to Graph Node.

## 2 Getting started overview
To deploy a GraphQL endpoint serving blockchain data to your Graph Node we will walk through the following steps:

1. [Create a subgraph project and manifest](#3-defining-your-subgraph)
1. [Define a GraphQL schema](#31-defining-your-graphql-schema)
1. [Define your source data](#32-defining-your-source-data)
1. [Generate types for your mapping with the Graph-CLI](#33-generate-types-for-your-mapping-with-the-graph-cli)
1. [Write your mappings](#34-write-your-mappings)
1. [Build and deploy your mappings to IPFS](#41-build-and-deploy-your-mappings-to-ipfs)
1. [Deploy your subgraph to your local Graph Node](#42-deploy-your-subgraph-to-your-local-graph-node)
1. [Query your newly deployed GraphQL API](#5-query-your-local-graph-node)
1. ???
1. [Profit!](#6-buidl-)

## 3 Defining your Subgraph
In The Graph, we refer to your project's GraphQL endpoint as a *subgraph*, because once deployed to the decentralized network, it will be just one subset of a global GraphQL endpoint.

The subgraph is defined as a YAML file called a *subgraph manifest*. See [here](https://github.com/graphprotocol/graph-cli/blob/master/examples/example-event-handler/subgraph.yaml) for an example, or [here](subgraph-manifest.md) for the full subgraph manifest specification. It comprises a schema, source data and mappings which are used to deploy your endpoint.

The subgraph manifest is typically placed into a *subgraph* directory. We'll also add a `package.json` and `tsconfig.json` in the directory, to take advantage of our Javascript-based build toolchain using the Graph CLI.

Before proceeding, follow the instructions in the [Graph CLI Readme](https://github.com/graphprotocol/graph-cli/) for setting up your subgraph directory.

### 3.1 Defining your GraphQL schema
GraphQL schemas are defined using the GraphQL interface definition language (IDL). If you've never written a GraphQL schema, we recommend checking out a [quick primer](https://graphql.org/learn/schema/#type-language) on the GraphQL type system.

With The Graph, you don't have to define your own top-level `Query` type, you simply define entity types, and Graph Node will generate top level fields for querying single instances and collections of that entity type. Each type that should be an entity is required to be annotated with an `@entity` directive.

##### Example
Define a simple Token entity type:
```graphql
type Token @entity {
  id: ID!
  name: String!
  minted: Int!
}
```

Later, when you've deployed your subgraph with this entity, you'll be able to query for Tokens:

```graphql
query {
  token(id: "123") {
    name
    minted
  }
}
```

or

```graphql
query {
  tokens(orderBy: minted) {
    id
    name
    minted
  }
}
```

See the [Schema API](graphql-api.md#3-schema) for a complete reference on defining your schema for The Graph.

Once you've completed your schema, add the path of the schema to the top level `schema` key in your subgraph manifest.

##### Example
```yaml
specVersion: 0.0.1
schema:
  file: ./schema.graphql
```

### 3.2 Defining your source data
Each data source in your subgraph is comprised of data on blockchain (i.e. an Ethereum smart contract) and a mapping which transforms and loads that data onto The Graph.

These are defined in the top-level `dataSources` key in the subgraph manifest.

##### Example
Defining a data source which is a smart contract implementing the ERC20 interface:
```yaml
dataSources:
- kind: ethereum/contract
  name: MyERC20Contract
  source:
    address: "f87e31492faf9a91b02ee0deaad50d51d56d5d4d"
    abi: ERC20
  mapping:
    kind: ethereum/events
    apiVersion: 0.0.1
    language: wasm/assemblyscript
    entities:
    - Parcel
    - ParcelData
    abis:
    - name: ERC20
      file: ./abis/ERC20ABI.json
    eventHandlers:
    - event: Transfer(address,address,uint)
      handler: handleTransfer
    file: ./mapping.ts
```

### 3.3 Generate types for your mapping with the Graph-CLI
In your subgraph directory, run the following command:
```shell
yarn run codegen
```

What this command does is it looks at the contract ABIs defined in your subgraph manifest, and for each datasource it generates TypeScript types (actually AssemblyScript types, but more on that later) for the smart contracts your mappings script will interface with, including the types of public methods and events.

This is incredibly useful for writing correct mappings, as well as improving developer productivity using the TypeScript language support in your favorite editor or IDE.

### 3.4 Write your mappings
Mappings are written in a subset of TypeScript called AssemblyScript which can be compiled down to WASM. AssemblyScript is stricter than normal TypeScript, yet provides a familiar syntax. A few TypeScript/Javascript features which are not supported in AssemblyScript include plain old Javascript objects (POJOs), untyped arrays, untyped maps, union types, the `any` type and variadic functions. `switch` statements also work differently. See [the AssemblyScript wiki](https://github.com/AssemblyScript/assemblyscript/wiki) for a full reference on AssemblyScript.

In your mapping file, create named export functions corresponding to the names specified in your subgraph manifest.

Each handler should accept a single parameter called `event` with a type corresponding to the name of the event which is being handled (this type was generated for you in the previous step).

##### Example
```typescript
export function handleTransfer(event: Transfer): void {
  // Event handler logic goes here
}
```

As mentioned, AssemblyScript does not have untyped maps or plain old Javascript objects, so to represent a collection of key value tuples with heterogeneous types, a global `Entity` type is included in the mapping types.

The `Entity` type has different setter methods for different types, satisfying AssemblyScript's requirement of strictly typed functions (and no union or `any` types).

#### Example
```typescript
let token = new Entity()
token.setString('name', "MyToken")
token.setAddress('owner', event.params.to)
token.setU256('amount', event.params.tokens)
```

There is also a global `store` module which has `set` and `get` methods for setting and getting the value(s) of a particular entity's attribute(s) in the store.


#### `store.set(entity: string, id: string, data: Entity)`

`store.set` expects the name of an entity type, the id of the entity and the `Entity` itself.

##### Example
```typescript
  store.set('Token', tokenId, token)
```

The eventHandlers functions return `void`. The only way that entities may be added to the Graph is by calling `store.set()`. `store.set()` may be called multiple times in an event handler.

**Note** `store.set()` will only set the entity attributes that have explicitly been set on the `Entity`. Attributes which are not explicitly set, or unset by calling `Entity.unset(<attribute>)`, will not be overwritten.

#### `store.get(entity: string, id: string)`

You can use `store.get` to retreive information previously added with `store.set`.
`store.get` expects the entity type and ID of the entity.

##### Example

```javascript
  store.set('Challenge', challengeId.toHex(), challenge)
  let challenge2 = store.get('Challenge', challengeId.toHex())
  let id = challenge2.getString("application")
  let registration = new Entity()
  registration.setString('challengeId', id)
  store.set('Registration', challengeId.toHex(), registration)
```


Along with `store.get()` you can use `.getString()`, `.getU256()`, etc... for getting property values from the entity.

## 4 Build

### 4.1 Build and deploy your mappings to IPFS
In order to deploy your subgraph to your Graph Node, the subgraph manifest will first need to be built and deployed to IPFS (along with all linked files).

Follow the instructions [here](https://ipfs.io/docs/getting-started/) to start a locally running IPFS daemon.

Once you've started your IPFS daemon you can run `yarn build-ipfs` in your subgraph directory (it assumes the default tcp port for the IPFS daemon).

This will compile your mappings, and deploy the mappings, schema and the subgraph manifest itself to IPFS. The result should be a single content hash.

You can pass that content hash into `ipfs cat` to view your subgraph manifest with files paths replaced by IPLD links.

### 4.2 Deploy your subgraph to your local Graph Node
Follow the instructions in the [Graph Node README](https://github.com/graphprotocol/graph-node) for deploying your subgraph to a locally running Graph Node using your subgraph's IPFS content hash.

## 5 Query your local Graph Node
With your subgraph deployed to your locally running Graph Node, visit http://127.0.0.1:8000/ to open up a [Graphiql](https://github.com/graphql/graphiql) interface where you can explore your deployed GraphQL API for your subgraph by issuing queries and viewing the schema.

See the [Query API](graphql-api.md#1-queries) for a complete reference on how to query your subgraph's entities.

#### Example
Query all `Token` entities:
```graphql
query {
  tokens {
    id
    owner
  }
}
```

## 6 Buidl 🚀
Start building world-changing dApps on top of your newly deployed GraphQL interface 🗿✨.

Feedback and contributions in the form of issues and pull requests are welcome!
