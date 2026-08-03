# LSP 3.17, trimmed to what the shim touches

Not a summary and not a paraphrase — every section below is the specification's
own text, unedited, with the rest removed. `specification.md` beside this file
is the whole thing and is the authority; this exists so that reading the
protocol costs a few thousand tokens rather than four hundred thousand.

**What is here, and why.** The shim is a proxy: it forwards every message it
does not answer, so it needs the base protocol exactly — framing, request and
response shapes, error codes, cancellation, ordering — and it needs document
synchronisation exactly, because it holds authoritative text for every open
document. It needs the definition family in full because that is the one thing
it answers itself.

**What is not here.** Every other language feature: completion, hover, code
actions, semantic tokens, inlay hints, formatting, rename, references, call and
type hierarchy, diagnostics, folding, symbols, notebooks, and the workspace and
window features. The shim forwards all of them byte-identically without
inspecting them (`shim.md` §3 argues how little inspection the forwarding path
needs), so their payload shapes are not its business. If a task ever needs one,
it is in `specification.md`.

**Generated** by `harness/reference-lsp`. Do not edit: regenerate.



<!-- ==================== The wire ==================== -->

## <a href="#baseProtocol" name="baseProtocol" class="anchor">Base Protocol</a>

The base protocol consists of a header and a content part (comparable to HTTP). The header and content part are
separated by a '\r\n'.

### <a href="#headerPart" name="headerPart" class="anchor">Header Part</a>

The header part consists of header fields. Each header field is comprised of a name and a value, separated by ': ' (a colon and a space). The structure of header fields conforms to the [HTTP semantic](https://tools.ietf.org/html/rfc7230#section-3.2). Each header field is terminated by '\r\n'. Considering the last header field and the overall header itself are each terminated with '\r\n', and that at least one header is mandatory, this means that two '\r\n' sequences always immediately precede the content part of a message.

Currently the following header fields are supported:

| Header Field Name | Value Type  | Description |
|:------------------|:------------|:------------|
| Content-Length    | number      | The length of the content part in bytes. This header is required. |
| Content-Type      | string      | The mime type of the content part. Defaults to application/vscode-jsonrpc; charset=utf-8 |
{: .table .table-bordered .table-responsive}

The header part is encoded using the 'ascii' encoding. This includes the '\r\n' separating the header and content part.

### <a href="#contentPart" name="contentPart" class="anchor">Content Part</a>

Contains the actual content of the message. The content part of a message uses [JSON-RPC](https://www.jsonrpc.org/) to describe requests, responses and notifications. The content part is encoded using the charset provided in the Content-Type field. It defaults to `utf-8`, which is the only encoding supported right now. If a server or client receives a header with a different encoding than `utf-8` it should respond with an error.

(Prior versions of the protocol used the string constant `utf8` which is not a correct encoding constant according to [specification](https://www.iana.org/assignments/character-sets/character-sets.xhtml).) For backwards compatibility it is highly recommended that a client and a server treat the string `utf8` as `utf-8`.

### Example:

```
Content-Length: ...\r\n
\r\n
{
	"jsonrpc": "2.0",
	"id": 1,
	"method": "textDocument/completion",
	"params": {
		...
	}
}
```
### Base Protocol JSON structures

The following TypeScript definitions describe the base [JSON-RPC protocol](https://www.jsonrpc.org/specification):

#### <a href="#baseTypes" name="baseTypes" class="anchor">Base Types</a>

The protocol uses the following definitions for integers, unsigned integers, decimal numbers, objects and arrays:

<div class="anchorHolder"><a href="#integer" name="integer" class="linkableAnchor"></a></div>

```typescript
/**
 * Defines an integer number in the range of -2^31 to 2^31 - 1.
 */
export type integer = number;
```

<div class="anchorHolder"><a href="#uinteger" name="uinteger" class="linkableAnchor"></a></div>

```typescript
/**
 * Defines an unsigned integer number in the range of 0 to 2^31 - 1.
 */
export type uinteger = number;
```

<div class="anchorHolder"><a href="#decimal" name="decimal" class="linkableAnchor"></a></div>

```typescript
/**
 * Defines a decimal number. Since decimal numbers are very
 * rare in the language server specification we denote the
 * exact range with every decimal using the mathematics
 * interval notation (e.g. [0, 1] denotes all decimals d with
 * 0 <= d <= 1.
 */
export type decimal = number;
```

<div class="anchorHolder"><a href="#lspAny" name="lspAny" class="linkableAnchor"></a></div>

```typescript
/**
 * The LSP any type
 *
 * @since 3.17.0
 */
export type LSPAny = LSPObject | LSPArray | string | integer | uinteger |
	decimal | boolean | null;
```

<div class="anchorHolder"><a href="#lspObject" name="lspObject" class="linkableAnchor"></a></div>

```typescript
/**
 * LSP object definition.
 *
 * @since 3.17.0
 */
export type LSPObject = { [key: string]: LSPAny };
```

<div class="anchorHolder"><a href="#lspArray" name="lspArray" class="linkableAnchor"></a></div>

```typescript
/**
 * LSP arrays.
 *
 * @since 3.17.0
 */
export type LSPArray = LSPAny[];
```

#### <a href="#abstractMessage" name="abstractMessage" class="anchor">Abstract Message</a>

A general message as defined by JSON-RPC. The language server protocol always uses "2.0" as the `jsonrpc` version.

<div class="anchorHolder"><a href="#message" name="message" class="linkableAnchor"></a></div>

```typescript
interface Message {
	jsonrpc: string;
}
```
#### <a href="#requestMessage" name="requestMessage" class="anchor">Request Message</a>

A request message to describe a request between the client and the server. Every processed request must send a response back to the sender of the request.

```typescript
interface RequestMessage extends Message {

	/**
	 * The request id.
	 */
	id: integer | string;

	/**
	 * The method to be invoked.
	 */
	method: string;

	/**
	 * The method's params.
	 */
	params?: array | object;
}
```

#### <a href="#responseMessage" name="responseMessage" class="anchor">Response Message</a>

A Response Message sent as a result of a request. If a request doesn't provide a result value the receiver of a request still needs to return a response message to conform to the JSON-RPC specification. The result property of the ResponseMessage should be set to `null` in this case to signal a successful request.

```typescript
interface ResponseMessage extends Message {
	/**
	 * The request id.
	 */
	id: integer | string | null;

	/**
	 * The result of a request. This member is REQUIRED on success.
	 * This member MUST NOT exist if there was an error invoking the method.
	 */
	result?: LSPAny;

	/**
	 * The error object in case a request fails.
	 */
	error?: ResponseError;
}
```

<div class="anchorHolder"><a href="#responseError" name="responseError" class="linkableAnchor"></a></div>

```typescript
interface ResponseError {
	/**
	 * A number indicating the error type that occurred.
	 */
	code: integer;

	/**
	 * A string providing a short description of the error.
	 */
	message: string;

	/**
	 * A primitive or structured value that contains additional
	 * information about the error. Can be omitted.
	 */
	data?: LSPAny;
}
```

<div class="anchorHolder"><a href="#errorCodes" name="errorCodes" class="linkableAnchor"></a></div>

```typescript
export namespace ErrorCodes {
	// Defined by JSON-RPC
	export const ParseError: integer = -32700;
	export const InvalidRequest: integer = -32600;
	export const MethodNotFound: integer = -32601;
	export const InvalidParams: integer = -32602;
	export const InternalError: integer = -32603;

	/**
	 * This is the start range of JSON-RPC reserved error codes.
	 * It doesn't denote a real error code. No LSP error codes should
	 * be defined between the start and end range. For backwards
	 * compatibility the `ServerNotInitialized` and the `UnknownErrorCode`
	 * are left in the range.
	 *
	 * @since 3.16.0
	 */
	export const jsonrpcReservedErrorRangeStart: integer = -32099;
	/** @deprecated use jsonrpcReservedErrorRangeStart */
	export const serverErrorStart: integer = jsonrpcReservedErrorRangeStart;

	/**
	 * Error code indicating that a server received a notification or
	 * request before the server received the `initialize` request.
	 */
	export const ServerNotInitialized: integer = -32002;
	export const UnknownErrorCode: integer = -32001;

	/**
	 * This is the end range of JSON-RPC reserved error codes.
	 * It doesn't denote a real error code.
	 *
	 * @since 3.16.0
	 */
	export const jsonrpcReservedErrorRangeEnd = -32000;
	/** @deprecated use jsonrpcReservedErrorRangeEnd */
	export const serverErrorEnd: integer = jsonrpcReservedErrorRangeEnd;

	/**
	 * This is the start range of LSP reserved error codes.
	 * It doesn't denote a real error code.
	 *
	 * @since 3.16.0
	 */
	export const lspReservedErrorRangeStart: integer = -32899;

	/**
	 * A request failed but it was syntactically correct, e.g the
	 * method name was known and the parameters were valid. The error
	 * message should contain human readable information about why
	 * the request failed.
	 *
	 * @since 3.17.0
	 */
	export const RequestFailed: integer = -32803;

	/**
	 * The server cancelled the request. This error code should
	 * only be used for requests that explicitly support being
	 * server cancellable.
	 *
	 * @since 3.17.0
	 */
	export const ServerCancelled: integer = -32802;

	/**
	 * The server detected that the content of a document got
	 * modified outside normal conditions. A server should
	 * NOT send this error code if it detects a content change
	 * in its unprocessed messages. The result even computed
	 * on an older state might still be useful for the client.
	 *
	 * If a client decides that a result is not of any use anymore
	 * the client should cancel the request.
	 */
	export const ContentModified: integer = -32801;

	/**
	 * The client has canceled a request and a server has detected
	 * the cancel.
	 */
	export const RequestCancelled: integer = -32800;

	/**
	 * This is the end range of LSP reserved error codes.
	 * It doesn't denote a real error code.
	 *
	 * @since 3.16.0
	 */
	export const lspReservedErrorRangeEnd: integer = -32800;
}
```
#### <a href="#notificationMessage" name="notificationMessage" class="anchor">Notification Message</a>

A notification message. A processed notification message must not send a response back. They work like events.

```typescript
interface NotificationMessage extends Message {
	/**
	 * The method to be invoked.
	 */
	method: string;

	/**
	 * The notification's params.
	 */
	params?: array | object;
}
```

#### <a href="#dollarRequests" name="dollarRequests" class="anchor">$ Notifications and Requests</a>

Notifications and requests whose methods start with '\$/' are messages which are protocol implementation dependent and might not be implementable in all clients or servers. For example if the server implementation uses a single threaded synchronous programming language then there is little a server can do to react to a `$/cancelRequest` notification. If a server or client receives notifications starting with '\$/' it is free to ignore the notification. If a server or client receives a request starting with '\$/' it must error the request with error code `MethodNotFound` (e.g. `-32601`).

#### <a href="#cancelRequest" name="cancelRequest" class="anchor">Cancellation Support (:arrow_right: :arrow_left:)</a>

The base protocol offers support for request cancellation. To cancel a request, a notification message with the following properties is sent:

_Notification_:
* method: '$/cancelRequest'
* params: `CancelParams` defined as follows:

```typescript
interface CancelParams {
	/**
	 * The request id to cancel.
	 */
	id: integer | string;
}
```

A request that got canceled still needs to return from the server and send a response back. It can not be left open / hanging. This is in line with the JSON-RPC protocol that requires that every request sends a response back. In addition it allows for returning partial results on cancel. If the request returns an error response on cancellation it is advised to set the error code to `ErrorCodes.RequestCancelled`.

#### <a href="#progress" name="progress" class="anchor">Progress Support (:arrow_right: :arrow_left:)</a>

> *Since version 3.15.0*

The base protocol offers also support to report progress in a generic fashion. This mechanism can be used to report any kind of progress including [work done progress](#workDoneProgress) (usually used to report progress in the user interface using a progress bar) and partial result progress to support streaming of results.

A progress notification has the following properties:

_Notification_:
* method: '$/progress'
* params: `ProgressParams` defined as follows:

```typescript
type ProgressToken = integer | string;
```

```typescript
interface ProgressParams<T> {
	/**
	 * The progress token provided by the client or server.
	 */
	token: ProgressToken;

	/**
	 * The progress data.
	 */
	value: T;
}
```

Progress is reported against a token. The token is different than the request ID which allows to report progress out of band and also for notification.

### <a href="#basicJsonStructures" name="basicJsonStructures" class="anchor">Basic JSON Structures</a>

There are quite some JSON structures that are shared between different requests and notifications. Their structure and capabilities are documented in this section.


#### <a href="#uri" name="uri" class="anchor">URI</a>

URI's are transferred as strings. The URI's format is defined in [https://tools.ietf.org/html/rfc3986](https://tools.ietf.org/html/rfc3986)

```
  foo://example.com:8042/over/there?name=ferret#nose
  \_/   \______________/\_________/ \_________/ \__/
   |           |            |            |        |
scheme     authority       path        query   fragment
   |   _____________________|__
  / \ /                        \
  urn:example:animal:ferret:nose
```

We also maintain a node module to parse a string into `scheme`, `authority`, `path`, `query`, and `fragment` URI components. The GitHub repository is [https://github.com/Microsoft/vscode-uri](https://github.com/Microsoft/vscode-uri), and the npm module is [https://www.npmjs.com/package/vscode-uri](https://www.npmjs.com/package/vscode-uri).

Many of the interfaces contain fields that correspond to the URI of a document. For clarity, the type of such a field is declared as a `DocumentUri`. Over the wire, it will still be transferred as a string, but this guarantees that the contents of that string can be parsed as a valid URI.

Care should be taken to handle encoding in URIs. For example, some clients (such as VS Code) may encode colons in drive letters while others do not. The URIs below are both valid, but clients and servers should be consistent with the form they use themselves to ensure the other party doesn't interpret them as distinct URIs. Clients and servers should not assume that each other are encoding the same way (for example a client encoding colons in drive letters cannot assume server responses will have encoded colons). The same applies to casing of drive letters - one party should not assume the other party will return paths with drive letters cased the same as itself.

```
file:///c:/project/readme.md
file:///C%3A/project/readme.md
```

<div class="anchorHolder"><a href="#documentUri" name="documentUri" class="linkableAnchor"></a></div>

```typescript
type DocumentUri = string;
```

There is also a tagging interface for normal non document URIs. It maps to a `string` as well.

```typescript
type URI = string;
```



#### <a href="#regExp" name="regExp" class="anchor">Regular Expressions</a>

Regular expression are a powerful tool and there are actual use cases for them in the language server protocol. However the downside with them is that almost every programming language has its own set of regular expression features so the specification can not simply refer to them as a regular expression. So the LSP uses a two step approach to support regular expressions:

* the client will announce which regular expression engine it will use. This will allow server that are written for a very specific client make full use of the regular expression capabilities of the client
* the specification will define a set of regular expression features that should be supported by a client. Instead of writing a new specification LSP will refer to the [ECMAScript Regular Expression specification](https://tc39.es/ecma262/#sec-regexp-regular-expression-objects) and remove features from it that are not necessary in the context of LSP or hard to implement for other clients.

_Client Capability_:

The following client capability is used to announce a client's regular expression engine

* property path (optional): `general.regularExpressions`
* property type: `RegularExpressionsClientCapabilities` defined as follows:

```typescript
/**
 * Client capabilities specific to regular expressions.
 */
export interface RegularExpressionsClientCapabilities {
	/**
	 * The engine's name.
	 */
	engine: string;

	/**
	 * The engine's version.
	 */
	version?: string;
}
```

The following table lists the well known engine values. Please note that the table should be driven by the community which integrates LSP into existing clients. It is not the goal of the spec to list all available regular expression engines.

Engine | Version | Documentation
------- | ------- | -------------
ECMAScript | `ES2020` | [ECMAScript 2020](https://tc39.es/ecma262/#sec-regexp-regular-expression-objects) & [MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Regular_Expressions)

_Regular Expression Subset_:

The following features from the [ECMAScript 2020](https://tc39.es/ecma262/#sec-regexp-regular-expression-objects) regular expression specification are NOT mandatory for a client:

- *Assertions*: Lookahead assertion, Negative lookahead assertion, lookbehind assertion, negative lookbehind assertion.
- *Character classes*: matching control characters using caret notation (e.g. `\cX`) and matching UTF-16 code units (e.g. `\uhhhh`).
- *Group and ranges*: named capturing groups.
- *Unicode property escapes*: none of the features needs to be supported.

The only regular expression flag that a client needs to support is 'i' to specify a case insensitive search.



#### <a href="#enumerations" name="enumerations" class="anchor">Enumerations</a>

The protocol supports two kind of enumerations: (a) integer based enumerations and (b) string based enumerations. Integer based enumerations usually start with `1`. The ones that don't are historical and they were kept to stay backwards compatible. If appropriate, the value set of an enumeration is announced by the defining side (e.g. client or server) and transmitted to the other side during the initialize handshake. An example is the `CompletionItemKind` enumeration. It is announced by the client using the `textDocument.completion.completionItemKind` client property.

To support the evolution of enumerations the using side of an enumeration shouldn't fail on an enumeration value it doesn't know. It should simply ignore it as a value it can use and try to do its best to preserve the value on round trips. Lets look at the `CompletionItemKind` enumeration as an example again: if in a future version of the specification an additional completion item kind with the value `n` gets added and announced by a client an (older) server not knowing about the value should not fail but simply ignore the value as a usable item kind.




#### <a href="#textDocuments" name="textDocuments" class="anchor">Text Documents</a>

The current protocol is tailored for textual documents whose content can be represented as a string. There is currently no support for binary documents. A position inside a document (see Position definition below) is expressed as a zero-based line and character offset.

> New in 3.17

Prior to 3.17 the offsets were always based on a UTF-16 string representation. So in a string of the form `a𐐀b` the character offset of the character `a` is 0, the character offset of `𐐀` is 1 and the character offset of b is 3 since `𐐀` is represented using two code units in UTF-16. Since 3.17 clients and servers can agree on a different string encoding representation (e.g. UTF-8). The client announces it's supported encoding via the client capability [`general.positionEncodings`](#clientCapabilities). The value is an array of position encodings the client supports, with decreasing preference (e.g. the encoding at index `0` is the most preferred one). To stay backwards compatible the only mandatory encoding is UTF-16 represented via the string `utf-16`. The server can pick one of the encodings offered by the client and signals that encoding back to the client via the initialize result's property [`capabilities.positionEncoding`](#serverCapabilities). If the string value `utf-16` is missing from the client's capability `general.positionEncodings` servers can safely assume that the client supports UTF-16. If the server omits the position encoding in its initialize result the encoding defaults to the string value `utf-16`. Implementation considerations: since the conversion from one encoding into another requires the content of the file / line the conversion is best done where the file is read which is usually on the server side.

To ensure that both client and server split the string into the same line representation the protocol specifies the following end-of-line sequences: '\n', '\r\n' and '\r'. Positions are line end character agnostic. So you can not specify a position that denotes `\r|\n` or `\n|` where `|` represents the character offset.

```typescript
export const EOL: string[] = ['\n', '\r\n', '\r'];
```



#### <a href="#position" name="position" class="anchor">Position</a>

Position in a text document expressed as zero-based line and zero-based character offset. A position is between two characters like an 'insert' cursor in an editor. Special values like for example `-1` to denote the end of a line are not supported.

```typescript
interface Position {
	/**
	 * Line position in a document (zero-based).
	 */
	line: uinteger;

	/**
	 * Character offset on a line in a document (zero-based). The meaning of this
	 * offset is determined by the negotiated `PositionEncodingKind`.
	 *
	 * If the character value is greater than the line length it defaults back
	 * to the line length.
	 */
	character: uinteger;
}
```

When describing positions the protocol needs to specify how offsets (specifically character offsets) should be interpreted.
The corresponding `PositionEncodingKind` is negotiated between the client and the server during initialization.

<div class="anchorHolder"><a href="#positionEncodingKind" name="positionEncodingKind" class="linkableAnchor"></a></div>


```typescript
/**
 * A type indicating how positions are encoded,
 * specifically what column offsets mean.
 *
 * @since 3.17.0
 */
export type PositionEncodingKind = string;

/**
 * A set of predefined position encoding kinds.
 *
 * @since 3.17.0
 */
export namespace PositionEncodingKind {

	/**
	 * Character offsets count UTF-8 code units (e.g bytes).
	 */
	export const UTF8: PositionEncodingKind = 'utf-8';

	/**
	 * Character offsets count UTF-16 code units.
	 *
	 * This is the default and must always be supported
	 * by servers
	 */
	export const UTF16: PositionEncodingKind = 'utf-16';

	/**
	 * Character offsets count UTF-32 code units.
	 *
	 * Implementation note: these are the same as Unicode code points,
	 * so this `PositionEncodingKind` may also be used for an
	 * encoding-agnostic representation of character offsets.
	 */
	export const UTF32: PositionEncodingKind = 'utf-32';
}
```



#### <a href="#range" name="range" class="anchor">Range</a>

A range in a text document expressed as (zero-based) start and end positions. A range is comparable to a selection in an editor. Therefore, the end position is exclusive. If you want to specify a range that contains a line including the line ending character(s) then use an end position denoting the start of the next line. For example:
```typescript
{
    start: { line: 5, character: 23 },
    end : { line: 6, character: 0 }
}
```

```typescript
interface Range {
	/**
	 * The range's start position.
	 */
	start: Position;

	/**
	 * The range's end position.
	 */
	end: Position;
}
```



#### <a href="#textDocumentItem" name="textDocumentItem" class="anchor">TextDocumentItem</a>

An item to transfer a text document from the client to the server.

```typescript
interface TextDocumentItem {
	/**
	 * The text document's URI.
	 */
	uri: DocumentUri;

	/**
	 * The text document's language identifier.
	 */
	languageId: string;

	/**
	 * The version number of this document (it will increase after each
	 * change, including undo/redo).
	 */
	version: integer;

	/**
	 * The content of the opened text document.
	 */
	text: string;
}
```

Text documents have a language identifier to identify a document on the server side when it handles more than one language to avoid re-interpreting the file extension. If a document refers to one of the programming languages listed below it is recommended that clients use those ids.

Language | Identifier
-------- | ----------
ABAP | `abap`
Windows Bat | `bat`
BibTeX | `bibtex`
Clojure | `clojure`
Coffeescript | `coffeescript`
C | `c`
C++ | `cpp`
C# | `csharp`
CSS | `css`
Diff | `diff`
Dart | `dart`
Dockerfile | `dockerfile`
Elixir | `elixir`
Erlang | `erlang`
F# | `fsharp`
Git | `git-commit` and `git-rebase`
Go | `go`
Groovy | `groovy`
Handlebars | `handlebars`
HTML | `html`
Ini | `ini`
Java | `java`
JavaScript | `javascript`
JavaScript React | `javascriptreact`
JSON | `json`
LaTeX | `latex`
Less | `less`
Lua | `lua`
Makefile | `makefile`
Markdown | `markdown`
Objective-C | `objective-c`
Objective-C++ | `objective-cpp`
Perl | `perl`
Perl 6 | `perl6`
PHP | `php`
Powershell | `powershell`
Pug | `jade`
Python | `python`
R | `r`
Razor (cshtml) | `razor`
Ruby | `ruby`
Rust | `rust`
SCSS | `scss` (syntax using curly brackets), `sass` (indented syntax)
Scala | `scala`
ShaderLab | `shaderlab`
Shell Script (Bash) | `shellscript`
SQL | `sql`
Swift | `swift`
TypeScript | `typescript`
TypeScript React| `typescriptreact`
TeX | `tex`
Visual Basic | `vb`
XML | `xml`
XSL | `xsl`
YAML | `yaml`
{: .table .table-bordered .table-responsive}



#### <a href="#textDocumentIdentifier" name="textDocumentIdentifier" class="anchor">TextDocumentIdentifier</a>

Text documents are identified using a URI. On the protocol level, URIs are passed as strings. The corresponding JSON structure looks like this:
```typescript
interface TextDocumentIdentifier {
	/**
	 * The text document's URI.
	 */
	uri: DocumentUri;
}
```



#### <a href="#versionedTextDocumentIdentifier" name="versionedTextDocumentIdentifier" class="anchor">VersionedTextDocumentIdentifier</a>

An identifier to denote a specific version of a text document. This information usually flows from the client to the server.

```typescript
interface VersionedTextDocumentIdentifier extends TextDocumentIdentifier {
	/**
	 * The version number of this document.
	 *
	 * The version number of a document will increase after each change,
	 * including undo/redo. The number doesn't need to be consecutive.
	 */
	version: integer;
}
```

An identifier which optionally denotes a specific version of a text document. This information usually flows from the server to the client.

<div class="anchorHolder"><a href="#optionalVersionedTextDocumentIdentifier" name="optionalVersionedTextDocumentIdentifier" class="linkableAnchor"></a></div>

```typescript
interface OptionalVersionedTextDocumentIdentifier extends TextDocumentIdentifier {
	/**
	 * The version number of this document. If an optional versioned text document
	 * identifier is sent from the server to the client and the file is not
	 * open in the editor (the server has not received an open notification
	 * before) the server can send `null` to indicate that the version is
	 * known and the content on disk is the master (as specified with document
	 * content ownership).
	 *
	 * The version number of a document will increase after each change,
	 * including undo/redo. The number doesn't need to be consecutive.
	 */
	version: integer | null;
}
```



#### <a href="#textDocumentPositionParams" name="textDocumentPositionParams" class="anchor">TextDocumentPositionParams</a>

Was `TextDocumentPosition` in 1.0 with inlined parameters.

A parameter literal used in requests to pass a text document and a position inside that document. It is up to the client to decide how a selection is converted into a position when issuing a request for a text document. The client can for example honor or ignore the selection direction to make LSP request consistent with features implemented internally.

```typescript
interface TextDocumentPositionParams {
	/**
	 * The text document.
	 */
	textDocument: TextDocumentIdentifier;

	/**
	 * The position inside the text document.
	 */
	position: Position;
}
```



#### <a href="#documentFilter" name="documentFilter" class="anchor">DocumentFilter</a>

A document filter denotes a document through properties like `language`, `scheme` or `pattern`. An example is a filter that applies to TypeScript files on disk. Another example is a filter that applies to JSON files with name `package.json`:
```typescript
{ language: 'typescript', scheme: 'file' }
{ language: 'json', pattern: '**/package.json' }
```

```typescript
export interface DocumentFilter {
	/**
	 * A language id, like `typescript`.
	 */
	language?: string;

	/**
	 * A Uri scheme, like `file` or `untitled`.
	 */
	scheme?: string;

	/**
	 * A glob pattern, like `*.{ts,js}`.
	 *
	 * Glob patterns can have the following syntax:
	 * - `*` to match zero or more characters in a path segment
	 * - `?` to match on one character in a path segment
	 * - `**` to match any number of path segments, including none
	 * - `{}` to group sub patterns into an OR expression. (e.g. `**​/*.{ts,js}`
	 *   matches all TypeScript and JavaScript files)
	 * - `[]` to declare a range of characters to match in a path segment
	 *   (e.g., `example.[0-9]` to match on `example.0`, `example.1`, …)
	 * - `[!...]` to negate a range of characters to match in a path segment
	 *   (e.g., `example.[!0-9]` to match on `example.a`, `example.b`, but
	 *   not `example.0`)
	 */
	pattern?: string;
}
```

Please note that for a document filter to be valid at least one of the properties for `language`, `scheme`, or `pattern` must be set. To keep the type definition simple all properties are marked as optional.

A document selector is the combination of one or more document filters.

<div class="anchorHolder"><a href="#documentSelector" name="documentSelector" class="linkableAnchor"></a></div>

```typescript
export type DocumentSelector = DocumentFilter[];
```




#### <a href="#textEdit" name="textEdit" class="anchor">TextEdit & AnnotatedTextEdit</a>

> New in version 3.16: Support for `AnnotatedTextEdit`.

A textual edit applicable to a text document.

```typescript
interface TextEdit {
	/**
	 * The range of the text document to be manipulated. To insert
	 * text into a document create a range where start === end.
	 */
	range: Range;

	/**
	 * The string to be inserted. For delete operations use an
	 * empty string.
	 */
	newText: string;
}
```
Since 3.16.0 there is also the concept of an annotated text edit which supports to add an annotation to a text edit. The annotation can add information describing the change to the text edit.

<div class="anchorHolder"><a href="#changeAnnotation" name="changeAnnotation" class="linkableAnchor"></a></div>

```typescript
/**
 * Additional information that describes document changes.
 *
 * @since 3.16.0
 */
export interface ChangeAnnotation {
	/**
	 * A human-readable string describing the actual change. The string
	 * is rendered prominent in the user interface.
	 */
	label: string;

	/**
	 * A flag which indicates that user confirmation is needed
	 * before applying the change.
	 */
	needsConfirmation?: boolean;

	/**
	 * A human-readable string which is rendered less prominent in
	 * the user interface.
	 */
	description?: string;
}
```

Usually clients provide options to group the changes along the annotations they are associated with. To support this in the protocol an edit or resource operation refers to a change annotation using an identifier and not the change annotation literal directly. This allows servers to use the identical annotation across multiple edits or resource operations which then allows clients to group the operations under that change annotation. The actual change annotations together with their identifiers are managed by the workspace edit via the new property `changeAnnotations`.

<div class="anchorHolder"><a href="#changeAnnotationIdentifier" name="changeAnnotationIdentifier" class="linkableAnchor"></a></div>

```typescript
/**
 * An identifier referring to a change annotation managed by a workspace
 * edit.
 *
 * @since 3.16.0.
 */
export type ChangeAnnotationIdentifier = string;
```

<div class="anchorHolder"><a href="#annotatedTextEdit" name="annotatedTextEdit" class="linkableAnchor"></a></div>

```typescript
/**
 * A special text edit with an additional change annotation.
 *
 * @since 3.16.0.
 */
export interface AnnotatedTextEdit extends TextEdit {
	/**
	 * The actual annotation identifier.
	 */
	annotationId: ChangeAnnotationIdentifier;
}
```



#### <a href="#textEditArray" name="textEditArray" class="anchor">TextEdit[]</a>

Complex text manipulations are described with an array of `TextEdit`'s or `AnnotatedTextEdit`'s, representing a single change to the document.

All text edits ranges refer to positions in the document they are computed on. They therefore move a document from state S1 to S2 without describing any intermediate state. Text edits ranges must never overlap, that means no part of the original document must be manipulated by more than one edit. However, it is possible that multiple edits have the same start position: multiple inserts, or any number of inserts followed by a single remove or replace edit. If multiple inserts have the same position, the order in the array defines the order in which the inserted strings appear in the resulting text.



#### <a href="#textDocumentEdit" name="textDocumentEdit" class="anchor">TextDocumentEdit</a>

> New in version 3.16: support for `AnnotatedTextEdit`. The support is guarded by the client capability `workspace.workspaceEdit.changeAnnotationSupport`. If a client doesn't signal the capability, servers shouldn't send `AnnotatedTextEdit` literals back to the client.

Describes textual changes on a single text document. The text document is referred to as a `OptionalVersionedTextDocumentIdentifier` to allow clients to check the text document version before an edit is applied. A `TextDocumentEdit` describes all changes on a version Si and after they are applied move the document to version Si+1. So the creator of a `TextDocumentEdit` doesn't need to sort the array of edits or do any kind of ordering. However the edits must be non overlapping.

```typescript
export interface TextDocumentEdit {
	/**
	 * The text document to change.
	 */
	textDocument: OptionalVersionedTextDocumentIdentifier;

	/**
	 * The edits to be applied.
	 *
	 * @since 3.16.0 - support for AnnotatedTextEdit. This is guarded by the
	 * client capability `workspace.workspaceEdit.changeAnnotationSupport`
	 */
	edits: (TextEdit | AnnotatedTextEdit)[];
}
```



#### <a href="#location" name="location" class="anchor">Location</a>

Represents a location inside a resource, such as a line inside a text file.
```typescript
interface Location {
	uri: DocumentUri;
	range: Range;
}
```



#### <a href="#locationLink" name="locationLink" class="anchor">LocationLink</a>

Represents a link between a source and a target location.

```typescript
interface LocationLink {

	/**
	 * Span of the origin of this link.
	 *
	 * Used as the underlined span for mouse interaction. Defaults to the word
	 * range at the mouse position.
	 */
	originSelectionRange?: Range;

	/**
	 * The target resource identifier of this link.
	 */
	targetUri: DocumentUri;

	/**
	 * The full target range of this link. If the target for example is a symbol
	 * then target range is the range enclosing this symbol not including
	 * leading/trailing whitespace but everything else like comments. This
	 * information is typically used to highlight the range in the editor.
	 */
	targetRange: Range;

	/**
	 * The range that should be selected and revealed when this link is being
	 * followed, e.g the name of a function. Must be contained by the
	 * `targetRange`. See also `DocumentSymbol#range`
	 */
	targetSelectionRange: Range;
}
```



#### <a href="#diagnostic" name="diagnostic" class="anchor">Diagnostic</a>

Represents a diagnostic, such as a compiler error or warning. Diagnostic objects are only valid in the scope of a resource.

```typescript
export interface Diagnostic {
	/**
	 * The range at which the message applies.
	 */
	range: Range;

	/**
	 * The diagnostic's severity. To avoid interpretation mismatches when a
	 * server is used with different clients it is highly recommended that
	 * servers always provide a severity value. If omitted, it’s recommended
	 * for the client to interpret it as an Error severity.
	 */
	severity?: DiagnosticSeverity;

	/**
	 * The diagnostic's code, which might appear in the user interface.
	 */
	code?: integer | string;

	/**
	 * An optional property to describe the error code.
	 *
	 * @since 3.16.0
	 */
	codeDescription?: CodeDescription;

	/**
	 * A human-readable string describing the source of this
	 * diagnostic, e.g. 'typescript' or 'super lint'.
	 */
	source?: string;

	/**
	 * The diagnostic's message.
	 */
	message: string;

	/**
	 * Additional metadata about the diagnostic.
	 *
	 * @since 3.15.0
	 */
	tags?: DiagnosticTag[];

	/**
	 * An array of related diagnostic information, e.g. when symbol-names within
	 * a scope collide all definitions can be marked via this property.
	 */
	relatedInformation?: DiagnosticRelatedInformation[];

	/**
	 * A data entry field that is preserved between a
	 * `textDocument/publishDiagnostics` notification and
	 * `textDocument/codeAction` request.
	 *
	 * @since 3.16.0
	 */
	data?: LSPAny;
}
```

The protocol currently supports the following diagnostic severities and tags:

<div class="anchorHolder"><a href="#diagnosticSeverity" name="diagnosticSeverity" class="linkableAnchor"></a></div>

```typescript
export namespace DiagnosticSeverity {
	/**
	 * Reports an error.
	 */
	export const Error: 1 = 1;
	/**
	 * Reports a warning.
	 */
	export const Warning: 2 = 2;
	/**
	 * Reports an information.
	 */
	export const Information: 3 = 3;
	/**
	 * Reports a hint.
	 */
	export const Hint: 4 = 4;
}

export type DiagnosticSeverity = 1 | 2 | 3 | 4;
```

<div class="anchorHolder"><a href="#diagnosticTag" name="diagnosticTag" class="linkableAnchor"></a></div>

```typescript
/**
 * The diagnostic tags.
 *
 * @since 3.15.0
 */
export namespace DiagnosticTag {
	/**
	 * Unused or unnecessary code.
	 *
	 * Clients are allowed to render diagnostics with this tag faded out
	 * instead of having an error squiggle.
	 */
	export const Unnecessary: 1 = 1;
	/**
	 * Deprecated or obsolete code.
	 *
	 * Clients are allowed to rendered diagnostics with this tag strike through.
	 */
	export const Deprecated: 2 = 2;
}

export type DiagnosticTag = 1 | 2;
```

`DiagnosticRelatedInformation` is defined as follows:

<div class="anchorHolder"><a href="#diagnosticRelatedInformation" name="diagnosticRelatedInformation" class="linkableAnchor"></a></div>

```typescript
/**
 * Represents a related message and source code location for a diagnostic.
 * This should be used to point to code locations that cause or are related to
 * a diagnostics, e.g when duplicating a symbol in a scope.
 */
export interface DiagnosticRelatedInformation {
	/**
	 * The location of this related diagnostic information.
	 */
	location: Location;

	/**
	 * The message of this related diagnostic information.
	 */
	message: string;
}
```

`CodeDescription` is defined as follows:

<div class="anchorHolder"><a href="#codeDescription" name="codeDescription" class="linkableAnchor"></a></div>

```typescript
/**
 * Structure to capture a description for an error code.
 *
 * @since 3.16.0
 */
export interface CodeDescription {
	/**
	 * An URI to open with more information about the diagnostic error.
	 */
	href: URI;
}
```



#### <a href="#command" name="command" class="anchor">Command</a>

Represents a reference to a command. Provides a title which will be used to represent a command in the UI. Commands are identified by a string identifier. The recommended way to handle commands is to implement their execution on the server side if the client and server provides the corresponding capabilities. Alternatively the tool extension code could handle the command. The protocol currently doesn't specify a set of well-known commands.

```typescript
interface Command {
	/**
	 * Title of the command, like `save`.
	 */
	title: string;
	/**
	 * The identifier of the actual command handler.
	 */
	command: string;
	/**
	 * Arguments that the command handler should be
	 * invoked with.
	 */
	arguments?: LSPAny[];
}
```



#### <a href="#markupContent" name="markupContent" class="anchor">MarkupContent</a>

 A `MarkupContent` literal represents a string value which content can be represented in different formats. Currently `plaintext` and `markdown` are supported formats. A `MarkupContent` is usually used in documentation properties of result literals like `CompletionItem` or `SignatureInformation`. If the format is `markdown` the content should follow the [GitHub Flavored Markdown Specification](https://github.github.com/gfm/).

```typescript
/**
 * Describes the content type that a client supports in various
 * result literals like `Hover`, `ParameterInfo` or `CompletionItem`.
 *
 * Please note that `MarkupKinds` must not start with a `$`. This kinds
 * are reserved for internal usage.
 */
export namespace MarkupKind {
	/**
	 * Plain text is supported as a content format
	 */
	export const PlainText: 'plaintext' = 'plaintext';

	/**
	 * Markdown is supported as a content format
	 */
	export const Markdown: 'markdown' = 'markdown';
}
export type MarkupKind = 'plaintext' | 'markdown';
```

<div class="anchorHolder"><a href="#markupContentDefinition" name="markupContentInnerDefinition" class="linkableAnchor"></a></div>

```typescript
/**
 * A `MarkupContent` literal represents a string value which content is
 * interpreted base on its kind flag. Currently the protocol supports
 * `plaintext` and `markdown` as markup kinds.
 *
 * If the kind is `markdown` then the value can contain fenced code blocks like
 * in GitHub issues.
 *
 * Here is an example how such a string can be constructed using
 * JavaScript / TypeScript:
 * ```typescript
 * let markdown: MarkdownContent = {
 * 	kind: MarkupKind.Markdown,
 * 	value: [
 * 		'# Header',
 * 		'Some text',
 * 		'```typescript',
 * 		'someCode();',
 * 		'```'
 * 	].join('\n')
 * };
 * ```
 *
 * *Please Note* that clients might sanitize the return markdown. A client could
 * decide to remove HTML from the markdown to avoid script execution.
 */
export interface MarkupContent {
	/**
	 * The type of the Markup
	 */
	kind: MarkupKind;

	/**
	 * The content itself
	 */
	value: string;
}
```

In addition clients should signal the markdown parser they are using via the client capability `general.markdown` introduced in version 3.16.0 defined as follows:

<div class="anchorHolder"><a href="#markdownClientCapabilities" name="markdownClientCapabilities" class="linkableAnchor"></a></div>

 ```typescript
/**
 * Client capabilities specific to the used markdown parser.
 *
 * @since 3.16.0
 */
export interface MarkdownClientCapabilities {
	/**
	 * The name of the parser.
	 */
	parser: string;

	/**
	 * The version of the parser.
	 */
	version?: string;

	/**
	 * A list of HTML tags that the client allows / supports in
	 * Markdown.
	 *
	 * @since 3.17.0
	 */
	allowedTags?: string[];
}
 ```

Known markdown parsers used by clients right now are:

Parser          | Version | Documentation
--------------- | ------- | -------------
marked          | 1.1.0   | [Marked Documentation](https://marked.js.org/)
Python-Markdown | 3.2.2   | [Python-Markdown Documentation](https://python-markdown.github.io)



<!-- ==================== How messages relate ==================== -->

### <a href="#capabilities" name="capabilities" class="anchor">Capabilities</a>

Not every language server can support all features defined by the protocol. LSP therefore provides ‘capabilities’. A capability groups a set of language features. A development tool and the language server announce their supported features using capabilities. As an example, a server announces that it can handle the `textDocument/hover` request, but it might not handle the `workspace/symbol` request. Similarly, a development tool announces its ability to provide `about to save` notifications before a document is saved, so that a server can compute textual edits to format the edited document before it is saved.

The set of capabilities is exchanged between the client and server during the [initialize](#initialize) request.

### <a href="#messageOrdering" name="messageOrdering" class="anchor">Request, Notification and Response Ordering</a>

Responses to requests should be sent in roughly the same order as the requests appear on the server or client side. So for example if a server receives a `textDocument/completion` request and then a `textDocument/signatureHelp` request it will usually first return the response for the `textDocument/completion` and then the response for `textDocument/signatureHelp`.

However, the server may decide to use a parallel execution strategy and may wish to return responses in a different order than the requests were received. The server may do so as long as this reordering doesn't affect the correctness of the responses. For example, reordering the result of `textDocument/completion` and `textDocument/signatureHelp` is allowed, as each of these requests usually won't affect the output of the other. On the other hand, the server most likely should not reorder `textDocument/definition` and `textDocument/rename` requests, since executing the latter may affect the result of the former.



<!-- ==================== Lifecycle ==================== -->

### <a href="#lifeCycleMessages" name="lifeCycleMessages" class="anchor">Server lifecycle</a>

The current protocol specification defines that the lifecycle of a server is managed by the client (e.g. a tool like VS Code or Emacs). It is up to the client to decide when to start (process-wise) and when to shutdown a server.


#### <a href="#initialize" name="initialize" class="anchor">Initialize Request (:leftwards_arrow_with_hook:)</a>

The initialize request is sent as the first request from the client to the server. If the server receives a request or notification before the `initialize` request it should act as follows:

* For a request the response should be an error with `code: -32002`. The message can be picked by the server.
* Notifications should be dropped, except for the exit notification. This will allow the exit of a server without an initialize request.

Until the server has responded to the `initialize` request with an `InitializeResult`, the client must not send any additional requests or notifications to the server. In addition the server is not allowed to send any requests or notifications to the client until it has responded with an `InitializeResult`, with the exception that during the `initialize` request the server is allowed to send the notifications `window/showMessage`, `window/logMessage` and `telemetry/event` as well as the `window/showMessageRequest` request to the client. In case the client sets up a progress token in the initialize params (e.g. property `workDoneToken`) the server is also allowed to use that token (and only that token) using the `$/progress` notification sent from the server to the client.

The `initialize` request may only be sent once.

_Request_:
* method: 'initialize'
* params: `InitializeParams` defined as follows:

<div class="anchorHolder"><a href="#initializeParams" name="initializeParams" class="linkableAnchor"></a></div>

```typescript
interface InitializeParams extends WorkDoneProgressParams {
	/**
	 * The process Id of the parent process that started the server. Is null if
	 * the process has not been started by another process. If the parent
	 * process is not alive then the server should exit (see exit notification)
	 * its process.
	 */
	processId: integer | null;

	/**
	 * Information about the client
	 *
	 * @since 3.15.0
	 */
	clientInfo?: {
		/**
		 * The name of the client as defined by the client.
		 */
		name: string;

		/**
		 * The client's version as defined by the client.
		 */
		version?: string;
	};

	/**
	 * The locale the client is currently showing the user interface
	 * in. This must not necessarily be the locale of the operating
	 * system.
	 *
	 * Uses IETF language tags as the value's syntax
	 * (See https://en.wikipedia.org/wiki/IETF_language_tag)
	 *
	 * @since 3.16.0
	 */
	locale?: string;

	/**
	 * The rootPath of the workspace. Is null
	 * if no folder is open.
	 *
	 * @deprecated in favour of `rootUri`.
	 */
	rootPath?: string | null;

	/**
	 * The rootUri of the workspace. Is null if no
	 * folder is open. If both `rootPath` and `rootUri` are set
	 * `rootUri` wins.
	 *
	 * @deprecated in favour of `workspaceFolders`
	 */
	rootUri: DocumentUri | null;

	/**
	 * User provided initialization options.
	 */
	initializationOptions?: LSPAny;

	/**
	 * The capabilities provided by the client (editor or tool)
	 */
	capabilities: ClientCapabilities;

	/**
	 * The initial trace setting. If omitted trace is disabled ('off').
	 */
	trace?: TraceValue;

	/**
	 * The workspace folders configured in the client when the server starts.
	 * This property is only available if the client supports workspace folders.
	 * It can be `null` if the client supports workspace folders but none are
	 * configured.
	 *
	 * @since 3.6.0
	 */
	workspaceFolders?: WorkspaceFolder[] | null;
}
```
Where `ClientCapabilities` and `TextDocumentClientCapabilities` are defined as follows:


##### TextDocumentClientCapabilities

`TextDocumentClientCapabilities` define capabilities the editor / tool provides on text documents.

<div class="anchorHolder"><a href="#textDocumentClientCapabilities" name="textDocumentClientCapabilities" class="linkableAnchor"></a></div>

```typescript
/**
 * Text document specific client capabilities.
 */
export interface TextDocumentClientCapabilities {

	synchronization?: TextDocumentSyncClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/completion` request.
	 */
	completion?: CompletionClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/hover` request.
	 */
	hover?: HoverClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/signatureHelp` request.
	 */
	signatureHelp?: SignatureHelpClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/declaration` request.
	 *
	 * @since 3.14.0
	 */
	declaration?: DeclarationClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/definition` request.
	 */
	definition?: DefinitionClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/typeDefinition` request.
	 *
	 * @since 3.6.0
	 */
	typeDefinition?: TypeDefinitionClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/implementation` request.
	 *
	 * @since 3.6.0
	 */
	implementation?: ImplementationClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/references` request.
	 */
	references?: ReferenceClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/documentHighlight` request.
	 */
	documentHighlight?: DocumentHighlightClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/documentSymbol` request.
	 */
	documentSymbol?: DocumentSymbolClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/codeAction` request.
	 */
	codeAction?: CodeActionClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/codeLens` request.
	 */
	codeLens?: CodeLensClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/documentLink` request.
	 */
	documentLink?: DocumentLinkClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/documentColor` and the
	 * `textDocument/colorPresentation` request.
	 *
	 * @since 3.6.0
	 */
	colorProvider?: DocumentColorClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/formatting` request.
	 */
	formatting?: DocumentFormattingClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/rangeFormatting` request.
	 */
	rangeFormatting?: DocumentRangeFormattingClientCapabilities;

	/** request.
	 * Capabilities specific to the `textDocument/onTypeFormatting` request.
	 */
	onTypeFormatting?: DocumentOnTypeFormattingClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/rename` request.
	 */
	rename?: RenameClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/publishDiagnostics`
	 * notification.
	 */
	publishDiagnostics?: PublishDiagnosticsClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/foldingRange` request.
	 *
	 * @since 3.10.0
	 */
	foldingRange?: FoldingRangeClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/selectionRange` request.
	 *
	 * @since 3.15.0
	 */
	selectionRange?: SelectionRangeClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/linkedEditingRange` request.
	 *
	 * @since 3.16.0
	 */
	linkedEditingRange?: LinkedEditingRangeClientCapabilities;

	/**
	 * Capabilities specific to the various call hierarchy requests.
	 *
	 * @since 3.16.0
	 */
	callHierarchy?: CallHierarchyClientCapabilities;

	/**
	 * Capabilities specific to the various semantic token requests.
	 *
	 * @since 3.16.0
	 */
	semanticTokens?: SemanticTokensClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/moniker` request.
	 *
	 * @since 3.16.0
	 */
	moniker?: MonikerClientCapabilities;

	/**
	 * Capabilities specific to the various type hierarchy requests.
	 *
	 * @since 3.17.0
	 */
	typeHierarchy?: TypeHierarchyClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/inlineValue` request.
	 *
	 * @since 3.17.0
	 */
	inlineValue?: InlineValueClientCapabilities;

	/**
	 * Capabilities specific to the `textDocument/inlayHint` request.
	 *
	 * @since 3.17.0
	 */
	inlayHint?: InlayHintClientCapabilities;

	/**
	 * Capabilities specific to the diagnostic pull model.
	 *
	 * @since 3.17.0
	 */
	diagnostic?: DiagnosticClientCapabilities;
}
```

##### NotebookDocumentClientCapabilities

`NotebookDocumentClientCapabilities` define capabilities the editor / tool provides on notebook documents.

<div class="anchorHolder"><a href="#notebookDocumentClientCapabilities" name="notebookDocumentClientCapabilities" class="linkableAnchor"></a></div>

```typescript
/**
 * Capabilities specific to the notebook document support.
 *
 * @since 3.17.0
 */
export interface NotebookDocumentClientCapabilities {
	/**
	 * Capabilities specific to notebook document synchronization
	 *
	 * @since 3.17.0
	 */
	synchronization: NotebookDocumentSyncClientCapabilities;
}
```


`ClientCapabilities` define capabilities for dynamic registration, workspace and text document features the client supports. The `experimental` can be used to pass experimental capabilities under development. For future compatibility a `ClientCapabilities` object literal can have more properties set than currently defined. Servers receiving a `ClientCapabilities` object literal with unknown properties should ignore these properties. A missing property should be interpreted as an absence of the capability. If a missing property normally defines sub properties, all missing sub properties should be interpreted as an absence of the corresponding capability.

Client capabilities got introduced with version 3.0 of the protocol. They therefore only describe capabilities that got introduced in 3.x or later. Capabilities that existed in the 2.x version of the protocol are still mandatory for clients. Clients cannot opt out of providing them. So even if a client omits the `ClientCapabilities.textDocument.synchronization` it is still required that the client provides text document synchronization (e.g. open, changed and close notifications).

<div class="anchorHolder"><a href="#clientCapabilities" name="clientCapabilities" class="linkableAnchor"></a></div>

```typescript
interface ClientCapabilities {
	/**
	 * Workspace specific client capabilities.
	 */
	workspace?: {
		/**
		 * The client supports applying batch edits
		 * to the workspace by supporting the request
		 * 'workspace/applyEdit'
		 */
		applyEdit?: boolean;

		/**
		 * Capabilities specific to `WorkspaceEdit`s
		 */
		workspaceEdit?: WorkspaceEditClientCapabilities;

		/**
		 * Capabilities specific to the `workspace/didChangeConfiguration`
		 * notification.
		 */
		didChangeConfiguration?: DidChangeConfigurationClientCapabilities;

		/**
		 * Capabilities specific to the `workspace/didChangeWatchedFiles`
		 * notification.
		 */
		didChangeWatchedFiles?: DidChangeWatchedFilesClientCapabilities;

		/**
		 * Capabilities specific to the `workspace/symbol` request.
		 */
		symbol?: WorkspaceSymbolClientCapabilities;

		/**
		 * Capabilities specific to the `workspace/executeCommand` request.
		 */
		executeCommand?: ExecuteCommandClientCapabilities;

		/**
		 * The client has support for workspace folders.
		 *
		 * @since 3.6.0
		 */
		workspaceFolders?: boolean;

		/**
		 * The client supports `workspace/configuration` requests.
		 *
		 * @since 3.6.0
		 */
		configuration?: boolean;

		/**
		 * Capabilities specific to the semantic token requests scoped to the
		 * workspace.
		 *
		 * @since 3.16.0
		 */
		 semanticTokens?: SemanticTokensWorkspaceClientCapabilities;

		/**
		 * Capabilities specific to the code lens requests scoped to the
		 * workspace.
		 *
		 * @since 3.16.0
		 */
		codeLens?: CodeLensWorkspaceClientCapabilities;

		/**
		 * The client has support for file requests/notifications.
		 *
		 * @since 3.16.0
		 */
		fileOperations?: {
			/**
			 * Whether the client supports dynamic registration for file
			 * requests/notifications.
			 */
			dynamicRegistration?: boolean;

			/**
			 * The client has support for sending didCreateFiles notifications.
			 */
			didCreate?: boolean;

			/**
			 * The client has support for sending willCreateFiles requests.
			 */
			willCreate?: boolean;

			/**
			 * The client has support for sending didRenameFiles notifications.
			 */
			didRename?: boolean;

			/**
			 * The client has support for sending willRenameFiles requests.
			 */
			willRename?: boolean;

			/**
			 * The client has support for sending didDeleteFiles notifications.
			 */
			didDelete?: boolean;

			/**
			 * The client has support for sending willDeleteFiles requests.
			 */
			willDelete?: boolean;
		};

		/**
		 * Client workspace capabilities specific to inline values.
		 *
		 * @since 3.17.0
		 */
		inlineValue?: InlineValueWorkspaceClientCapabilities;

		/**
		 * Client workspace capabilities specific to inlay hints.
		 *
		 * @since 3.17.0
		 */
		inlayHint?: InlayHintWorkspaceClientCapabilities;

		/**
		 * Client workspace capabilities specific to diagnostics.
		 *
		 * @since 3.17.0.
		 */
		diagnostics?: DiagnosticWorkspaceClientCapabilities;
	};

	/**
	 * Text document specific client capabilities.
	 */
	textDocument?: TextDocumentClientCapabilities;

	/**
	 * Capabilities specific to the notebook document support.
	 *
	 * @since 3.17.0
	 */
	notebookDocument?: NotebookDocumentClientCapabilities;

	/**
	 * Window specific client capabilities.
	 */
	window?: {
		/**
		 * It indicates whether the client supports server initiated
		 * progress using the `window/workDoneProgress/create` request.
		 *
		 * The capability also controls Whether client supports handling
		 * of progress notifications. If set servers are allowed to report a
		 * `workDoneProgress` property in the request specific server
		 * capabilities.
		 *
		 * @since 3.15.0
		 */
		workDoneProgress?: boolean;

		/**
		 * Capabilities specific to the showMessage request
		 *
		 * @since 3.16.0
		 */
		showMessage?: ShowMessageRequestClientCapabilities;

		/**
		 * Client capabilities for the show document request.
		 *
		 * @since 3.16.0
		 */
		showDocument?: ShowDocumentClientCapabilities;
	};

	/**
	 * General client capabilities.
	 *
	 * @since 3.16.0
	 */
	general?: {
		/**
		 * Client capability that signals how the client
		 * handles stale requests (e.g. a request
		 * for which the client will not process the response
		 * anymore since the information is outdated).
		 *
		 * @since 3.17.0
		 */
		staleRequestSupport?: {
			/**
			 * The client will actively cancel the request.
			 */
			cancel: boolean;

			/**
			 * The list of requests for which the client
			 * will retry the request if it receives a
			 * response with error code `ContentModified``
			 */
			 retryOnContentModified: string[];
		}

		/**
		 * Client capabilities specific to regular expressions.
		 *
		 * @since 3.16.0
		 */
		regularExpressions?: RegularExpressionsClientCapabilities;

		/**
		 * Client capabilities specific to the client's markdown parser.
		 *
		 * @since 3.16.0
		 */
		markdown?: MarkdownClientCapabilities;

		/**
		 * The position encodings supported by the client. Client and server
		 * have to agree on the same position encoding to ensure that offsets
		 * (e.g. character position in a line) are interpreted the same on both
		 * side.
		 *
		 * To keep the protocol backwards compatible the following applies: if
		 * the value 'utf-16' is missing from the array of position encodings
		 * servers can assume that the client supports UTF-16. UTF-16 is
		 * therefore a mandatory encoding.
		 *
		 * If omitted it defaults to ['utf-16'].
		 *
		 * Implementation considerations: since the conversion from one encoding
		 * into another requires the content of the file / line the conversion
		 * is best done where the file is read which is usually on the server
		 * side.
		 *
		 * @since 3.17.0
		 */
		positionEncodings?: PositionEncodingKind[];
	};

	/**
	 * Experimental client capabilities.
	 */
	experimental?: LSPAny;
}
```

_Response_:
* result: `InitializeResult` defined as follows:

<div class="anchorHolder"><a href="#initializeResult" name="initializeResult" class="linkableAnchor"></a></div>

```typescript
interface InitializeResult {
	/**
	 * The capabilities the language server provides.
	 */
	capabilities: ServerCapabilities;

	/**
	 * Information about the server.
	 *
	 * @since 3.15.0
	 */
	serverInfo?: {
		/**
		 * The name of the server as defined by the server.
		 */
		name: string;

		/**
		 * The server's version as defined by the server.
		 */
		version?: string;
	};
}
```
* error.code:

<div class="anchorHolder"><a href="#initializeErrorCodes" name="initializeErrorCodes" class="linkableAnchor"></a></div>

```typescript
/**
 * Known error codes for an `InitializeErrorCodes`;
 */
export namespace InitializeErrorCodes {

	/**
	 * If the protocol version provided by the client can't be handled by
	 * the server.
	 *
	 * @deprecated This initialize error got replaced by client capabilities.
	 * There is no version handshake in version 3.0x
	 */
	export const unknownProtocolVersion: 1 = 1;
}

export type InitializeErrorCodes = 1;
```

* error.data:

<div class="anchorHolder"><a href="#initializeError" name="initializeError" class="linkableAnchor"></a></div>

```typescript
interface InitializeError {
	/**
	 * Indicates whether the client execute the following retry logic:
	 * (1) show the message provided by the ResponseError to the user
	 * (2) user selects retry or cancel
	 * (3) if user selected retry the initialize method is sent again.
	 */
	retry: boolean;
}
```

The server can signal the following capabilities:

<div class="anchorHolder"><a href="#serverCapabilities" name="serverCapabilities" class="linkableAnchor"></a></div>

```typescript
interface ServerCapabilities {

	/**
	 * The position encoding the server picked from the encodings offered
	 * by the client via the client capability `general.positionEncodings`.
	 *
	 * If the client didn't provide any position encodings the only valid
	 * value that a server can return is 'utf-16'.
	 *
	 * If omitted it defaults to 'utf-16'.
	 *
	 * @since 3.17.0
	 */
	positionEncoding?: PositionEncodingKind;

	/**
	 * Defines how text documents are synced. Is either a detailed structure
	 * defining each notification or for backwards compatibility the
	 * TextDocumentSyncKind number. If omitted it defaults to
	 * `TextDocumentSyncKind.None`.
	 */
	textDocumentSync?: TextDocumentSyncOptions | TextDocumentSyncKind;

	/**
	 * Defines how notebook documents are synced.
	 *
	 * @since 3.17.0
	 */
	notebookDocumentSync?: NotebookDocumentSyncOptions
		| NotebookDocumentSyncRegistrationOptions;

	/**
	 * The server provides completion support.
	 */
	completionProvider?: CompletionOptions;

	/**
	 * The server provides hover support.
	 */
	hoverProvider?: boolean | HoverOptions;

	/**
	 * The server provides signature help support.
	 */
	signatureHelpProvider?: SignatureHelpOptions;

	/**
	 * The server provides go to declaration support.
	 *
	 * @since 3.14.0
	 */
	declarationProvider?: boolean | DeclarationOptions
		| DeclarationRegistrationOptions;

	/**
	 * The server provides goto definition support.
	 */
	definitionProvider?: boolean | DefinitionOptions;

	/**
	 * The server provides goto type definition support.
	 *
	 * @since 3.6.0
	 */
	typeDefinitionProvider?: boolean | TypeDefinitionOptions
		| TypeDefinitionRegistrationOptions;

	/**
	 * The server provides goto implementation support.
	 *
	 * @since 3.6.0
	 */
	implementationProvider?: boolean | ImplementationOptions
		| ImplementationRegistrationOptions;

	/**
	 * The server provides find references support.
	 */
	referencesProvider?: boolean | ReferenceOptions;

	/**
	 * The server provides document highlight support.
	 */
	documentHighlightProvider?: boolean | DocumentHighlightOptions;

	/**
	 * The server provides document symbol support.
	 */
	documentSymbolProvider?: boolean | DocumentSymbolOptions;

	/**
	 * The server provides code actions. The `CodeActionOptions` return type is
	 * only valid if the client signals code action literal support via the
	 * property `textDocument.codeAction.codeActionLiteralSupport`.
	 */
	codeActionProvider?: boolean | CodeActionOptions;

	/**
	 * The server provides code lens.
	 */
	codeLensProvider?: CodeLensOptions;

	/**
	 * The server provides document link support.
	 */
	documentLinkProvider?: DocumentLinkOptions;

	/**
	 * The server provides color provider support.
	 *
	 * @since 3.6.0
	 */
	colorProvider?: boolean | DocumentColorOptions
		| DocumentColorRegistrationOptions;

	/**
	 * The server provides document formatting.
	 */
	documentFormattingProvider?: boolean | DocumentFormattingOptions;

	/**
	 * The server provides document range formatting.
	 */
	documentRangeFormattingProvider?: boolean | DocumentRangeFormattingOptions;

	/**
	 * The server provides document formatting on typing.
	 */
	documentOnTypeFormattingProvider?: DocumentOnTypeFormattingOptions;

	/**
	 * The server provides rename support. RenameOptions may only be
	 * specified if the client states that it supports
	 * `prepareSupport` in its initial `initialize` request.
	 */
	renameProvider?: boolean | RenameOptions;

	/**
	 * The server provides folding provider support.
	 *
	 * @since 3.10.0
	 */
	foldingRangeProvider?: boolean | FoldingRangeOptions
		| FoldingRangeRegistrationOptions;

	/**
	 * The server provides execute command support.
	 */
	executeCommandProvider?: ExecuteCommandOptions;

	/**
	 * The server provides selection range support.
	 *
	 * @since 3.15.0
	 */
	selectionRangeProvider?: boolean | SelectionRangeOptions
		| SelectionRangeRegistrationOptions;

	/**
	 * The server provides linked editing range support.
	 *
	 * @since 3.16.0
	 */
	linkedEditingRangeProvider?: boolean | LinkedEditingRangeOptions
		| LinkedEditingRangeRegistrationOptions;

	/**
	 * The server provides call hierarchy support.
	 *
	 * @since 3.16.0
	 */
	callHierarchyProvider?: boolean | CallHierarchyOptions
		| CallHierarchyRegistrationOptions;

	/**
	 * The server provides semantic tokens support.
	 *
	 * @since 3.16.0
	 */
	semanticTokensProvider?: SemanticTokensOptions
		| SemanticTokensRegistrationOptions;

	/**
	 * Whether server provides moniker support.
	 *
	 * @since 3.16.0
	 */
	monikerProvider?: boolean | MonikerOptions | MonikerRegistrationOptions;

	/**
	 * The server provides type hierarchy support.
	 *
	 * @since 3.17.0
	 */
	typeHierarchyProvider?: boolean | TypeHierarchyOptions
		 | TypeHierarchyRegistrationOptions;

	/**
	 * The server provides inline values.
	 *
	 * @since 3.17.0
	 */
	inlineValueProvider?: boolean | InlineValueOptions
		 | InlineValueRegistrationOptions;

	/**
	 * The server provides inlay hints.
	 *
	 * @since 3.17.0
	 */
	inlayHintProvider?: boolean | InlayHintOptions
		 | InlayHintRegistrationOptions;

	/**
	 * The server has support for pull model diagnostics.
	 *
	 * @since 3.17.0
	 */
	diagnosticProvider?: DiagnosticOptions | DiagnosticRegistrationOptions;

	/**
	 * The server provides workspace symbol support.
	 */
	workspaceSymbolProvider?: boolean | WorkspaceSymbolOptions;

	/**
	 * Workspace specific server capabilities
	 */
	workspace?: {
		/**
		 * The server supports workspace folder.
		 *
		 * @since 3.6.0
		 */
		workspaceFolders?: WorkspaceFoldersServerCapabilities;

		/**
		 * The server is interested in file notifications/requests.
		 *
		 * @since 3.16.0
		 */
		fileOperations?: {
			/**
			 * The server is interested in receiving didCreateFiles
			 * notifications.
			 */
			didCreate?: FileOperationRegistrationOptions;

			/**
			 * The server is interested in receiving willCreateFiles requests.
			 */
			willCreate?: FileOperationRegistrationOptions;

			/**
			 * The server is interested in receiving didRenameFiles
			 * notifications.
			 */
			didRename?: FileOperationRegistrationOptions;

			/**
			 * The server is interested in receiving willRenameFiles requests.
			 */
			willRename?: FileOperationRegistrationOptions;

			/**
			 * The server is interested in receiving didDeleteFiles file
			 * notifications.
			 */
			didDelete?: FileOperationRegistrationOptions;

			/**
			 * The server is interested in receiving willDeleteFiles file
			 * requests.
			 */
			willDelete?: FileOperationRegistrationOptions;
		};
	};

	/**
	 * Experimental server capabilities.
	 */
	experimental?: LSPAny;
}
```



#### <a href="#initialized" name="initialized" class="anchor">Initialized Notification (:arrow_right:)</a>

The initialized notification is sent from the client to the server after the client received the result of the `initialize` request but before the client is sending any other request or notification to the server. The server can use the `initialized` notification, for example, to dynamically register capabilities. The `initialized` notification may only be sent once.

_Notification_:
* method: 'initialized'
* params: `InitializedParams` defined as follows:

```typescript
interface InitializedParams {
}
```



#### <a href="#client_registerCapability" name="client_registerCapability" class="anchor">Register Capability (:arrow_right_hook:)</a>

The `client/registerCapability` request is sent from the server to the client to register for a new capability on the client side. Not all clients need to support dynamic capability registration. A client opts in via the `dynamicRegistration` property on the specific client capabilities. A client can even provide dynamic registration for capability A but not for capability B (see `TextDocumentClientCapabilities` as an example).

Server must not register the same capability both statically through the initialize result and dynamically for the same document selector. If a server wants to support both static and dynamic registration it needs to check the client capability in the initialize request and only register the capability statically if the client doesn't support dynamic registration for that capability.

_Request_:
* method: 'client/registerCapability'
* params: `RegistrationParams`

Where `RegistrationParams` are defined as follows:

<div class="anchorHolder"><a href="#registration" name="registration" class="linkableAnchor"></a></div>

```typescript
/**
 * General parameters to register for a capability.
 */
export interface Registration {
	/**
	 * The id used to register the request. The id can be used to deregister
	 * the request again.
	 */
	id: string;

	/**
	 * The method / capability to register for.
	 */
	method: string;

	/**
	 * Options necessary for the registration.
	 */
	registerOptions?: LSPAny;
}
```

<div class="anchorHolder"><a href="#registrationParams" name="registrationParams" class="linkableAnchor"></a></div>

```typescript
export interface RegistrationParams {
	registrations: Registration[];
}
```

Since most of the registration options require to specify a document selector there is a base interface that can be used. See `TextDocumentRegistrationOptions`.

An example JSON-RPC message to register dynamically for the `textDocument/willSaveWaitUntil` feature on the client side is as follows (only details shown):

```json
{
	"method": "client/registerCapability",
	"params": {
		"registrations": [
			{
				"id": "79eee87c-c409-4664-8102-e03263673f6f",
				"method": "textDocument/willSaveWaitUntil",
				"registerOptions": {
					"documentSelector": [
						{ "language": "javascript" }
					]
				}
			}
		]
	}
}
```

This message is sent from the server to the client and after the client has successfully executed the request further `textDocument/willSaveWaitUntil` requests for JavaScript text documents are sent from the client to the server.

_Response_:
* result: void.
* error: code and message set in case an exception happens during the request.

`StaticRegistrationOptions` can be used to register a feature in the initialize result with a given server control ID to be able to un-register the feature later on.

<div class="anchorHolder"><a href="#staticRegistrationOptions" name="staticRegistrationOptions" class="linkableAnchor"></a></div>

```typescript
/**
 * Static registration options to be returned in the initialize request.
 */
export interface StaticRegistrationOptions {
	/**
	 * The id used to register the request. The id can be used to deregister
	 * the request again. See also Registration#id.
	 */
	id?: string;
}
```

`TextDocumentRegistrationOptions` can be used to dynamically register for requests for a set of text documents.

<div class="anchorHolder"><a href="#textDocumentRegistrationOptions" name="textDocumentRegistrationOptions" class="linkableAnchor"></a></div>

```typescript
/**
 * General text document registration options.
 */
export interface TextDocumentRegistrationOptions {
	/**
	 * A document selector to identify the scope of the registration. If set to
	 * null the document selector provided on the client side will be used.
	 */
	documentSelector: DocumentSelector | null;
}
```



#### <a href="#client_unregisterCapability" name="client_unregisterCapability" class="anchor">Unregister Capability (:arrow_right_hook:)</a>

The `client/unregisterCapability` request is sent from the server to the client to unregister a previously registered capability.

_Request_:
* method: 'client/unregisterCapability'
* params: `UnregistrationParams`

Where `UnregistrationParams` are defined as follows:

<div class="anchorHolder"><a href="#unregistration" name="unregistration" class="linkableAnchor"></a></div>

```typescript
/**
 * General parameters to unregister a capability.
 */
export interface Unregistration {
	/**
	 * The id used to unregister the request or notification. Usually an id
	 * provided during the register request.
	 */
	id: string;

	/**
	 * The method / capability to unregister for.
	 */
	method: string;
}
```

<div class="anchorHolder"><a href="#unregistrationParams" name="unregistrationParams" class="linkableAnchor"></a></div>

```typescript
export interface UnregistrationParams {
	// This should correctly be named `unregistrations`. However changing this
	// is a breaking change and needs to wait until we deliver a 4.x version
	// of the specification.
	unregisterations: Unregistration[];
}
```

An example JSON-RPC message to unregister the above registered `textDocument/willSaveWaitUntil` feature looks like this:

```json
{
	"method": "client/unregisterCapability",
	"params": {
		"unregisterations": [
			{
				"id": "79eee87c-c409-4664-8102-e03263673f6f",
				"method": "textDocument/willSaveWaitUntil"
			}
		]
	}
}
```
_Response_:
* result: void.
* error: code and message set in case an exception happens during the request.



#### <a href="#setTrace" name="setTrace" class="anchor">SetTrace Notification (:arrow_right:)</a>

A notification that should be used by the client to modify the trace setting of the server.

_Notification_:
* method: '$/setTrace'
* params: `SetTraceParams` defined as follows:

```typescript
interface SetTraceParams {
	/**
	 * The new value that should be assigned to the trace setting.
	 */
	value: TraceValue;
}
```



#### <a href="#logTrace" name="logTrace" class="anchor">LogTrace Notification (:arrow_left:)</a>

A notification to log the trace of the server's execution.
The amount and content of these notifications depends on the current `trace` configuration.
If `trace` is `'off'`, the server should not send any `logTrace` notification.
If `trace` is `'messages'`, the server should not add the `'verbose'` field in the `LogTraceParams`.

`$/logTrace` should be used for systematic trace reporting. For single debugging messages, the server should send [`window/logMessage`](#window_logMessage) notifications.


_Notification_:
* method: '$/logTrace'
* params: `LogTraceParams` defined as follows:

```typescript
interface LogTraceParams {
	/**
	 * The message to be logged.
	 */
	message: string;
	/**
	 * Additional information that can be computed if the `trace` configuration
	 * is set to `'verbose'`
	 */
	verbose?: string;
}
```



#### <a href="#shutdown" name="shutdown" class="anchor">Shutdown Request (:leftwards_arrow_with_hook:)</a>

The shutdown request is sent from the client to the server. It asks the server to shut down, but to not exit (otherwise the response might not be delivered correctly to the client). There is a separate exit notification that asks the server to exit. Clients must not send any notifications other than `exit` or requests to a server to which they have sent a shutdown request. Clients should also wait with sending the `exit` notification until they have received a response from the `shutdown` request.

If a server receives requests after a shutdown request those requests should error with `InvalidRequest`.

_Request_:
* method: 'shutdown'
* params: none

_Response_:
* result: null
* error: code and message set in case an exception happens during shutdown request.



#### <a href="#exit" name="exit" class="anchor">Exit Notification (:arrow_right:)</a>

A notification to ask the server to exit its process.
The server should exit with `success` code 0 if the shutdown request has been received before; otherwise with `error` code 1.

_Notification_:
* method: 'exit'
* params: none



<!-- ==================== Document synchronisation ==================== -->

### <a href="#textDocument_synchronization" name="textDocument_synchronization" class="anchor">Text Document Synchronization</a>

Client support for `textDocument/didOpen`, `textDocument/didChange` and `textDocument/didClose` notifications is mandatory in the protocol and clients can not opt out supporting them. This includes both full and incremental synchronization in the `textDocument/didChange` notification. In addition a server must either implement all three of them or none. Their capabilities are therefore controlled via a combined client and server capability. Opting out of text document synchronization makes only sense if the documents shown by the client are read only. Otherwise the server might receive request for documents, for which the content is managed in the client (e.g. they might have changed).

<a href="#textDocument_synchronization_cc" name="textDocument_synchronization_cc" class="anchor">_Client Capability_:</a>
* property path (optional): `textDocument.synchronization.dynamicRegistration`
* property type: `boolean`

Controls whether text document synchronization supports dynamic registration.

<a href="#textDocument_synchronization_sc" name="textDocument_synchronization_sc" class="anchor">_Server Capability_:</a>
* property path (optional): `textDocumentSync`
* property type: `TextDocumentSyncKind | TextDocumentSyncOptions`. The below definition of the `TextDocumentSyncOptions` only covers the properties specific to the open, change and close notifications. A complete definition covering all properties can be found [here](#textDocument_didClose):

<div class="anchorHolder"><a href="#textDocumentSyncKind" name="textDocumentSyncKind" class="linkableAnchor"></a></div>

```typescript
/**
 * Defines how the host (editor) should sync document changes to the language
 * server.
 */
export namespace TextDocumentSyncKind {
	/**
	 * Documents should not be synced at all.
	 */
	export const None = 0;

	/**
	 * Documents are synced by always sending the full content
	 * of the document.
	 */
	export const Full = 1;

	/**
	 * Documents are synced by sending the full content on open.
	 * After that only incremental updates to the document are
	 * sent.
	 */
	export const Incremental = 2;
}

export type TextDocumentSyncKind = 0 | 1 | 2;
```

<div class="anchorHolder"><a href="#textDocumentSyncOptions" name="textDocumentSyncOptions" class="linkableAnchor"></a></div>

```typescript
export interface TextDocumentSyncOptions {
	/**
	 * Open and close notifications are sent to the server. If omitted open
	 * close notifications should not be sent.
	 */
	openClose?: boolean;

	/**
	 * Change notifications are sent to the server. See
	 * TextDocumentSyncKind.None, TextDocumentSyncKind.Full and
	 * TextDocumentSyncKind.Incremental. If omitted it defaults to
	 * TextDocumentSyncKind.None.
	 */
	change?: TextDocumentSyncKind;
}
```


#### <a href="#textDocument_didOpen" name="textDocument_didOpen" class="anchor">DidOpenTextDocument Notification (:arrow_right:)</a>

The document open notification is sent from the client to the server to signal newly opened text documents. The document's content is now managed by the client and the server must not try to read the document's content using the document's Uri. Open in this sense means it is managed by the client. It doesn't necessarily mean that its content is presented in an editor. An open notification must not be sent more than once without a corresponding close notification send before. This means open and close notification must be balanced and the max open count for a particular textDocument is one. Note that a server's ability to fulfill requests is independent of whether a text document is open or closed.

The `DidOpenTextDocumentParams` contain the language id the document is associated with. If the language id of a document changes, the client needs to send a `textDocument/didClose` to the server followed by a `textDocument/didOpen` with the new language id if the server handles the new language id as well.

_Client Capability_:
See general synchronization [client capabilities](#textDocument_synchronization_cc).

_Server Capability_:
See general synchronization [server capabilities](#textDocument_synchronization_sc).

_Registration Options_: [`TextDocumentRegistrationOptions`](#textDocumentRegistrationOptions)

_Notification_:
* method: 'textDocument/didOpen'
* params: `DidOpenTextDocumentParams` defined as follows:

<div class="anchorHolder"><a href="#didOpenTextDocumentParams" name="didOpenTextDocumentParams" class="linkableAnchor"></a></div>

```typescript
interface DidOpenTextDocumentParams {
	/**
	 * The document that was opened.
	 */
	textDocument: TextDocumentItem;
}
```



#### <a href="#textDocument_didChange" name="textDocument_didChange" class="anchor">DidChangeTextDocument Notification (:arrow_right:)</a>

The document change notification is sent from the client to the server to signal changes to a text document. Before a client can change a text document it must claim ownership of its content using the `textDocument/didOpen` notification. In 2.0 the shape of the params has changed to include proper version numbers.

Before requesting information from the server (e.g., `textDocument/completion` or `textDocument/signatureHelp`), the client must ensure that the document's state is synchronized with the server to guarantee reliable results.

The following example shows how the client should synchronize the state when the user has continuous input, assuming user input triggered `textDocument/completion`:

| Document Version | User Input          | Client Behavior                                 | Request                   |
| ---------------- | ------------------- | ----------------------------------------------- | ------------------------- |
| 5                | document change one | sync document `v5` to the server                | `textDocument/didChange`  |
| 5                | -                   | request from the server, based on document `v5` | `textDocument/completion` |
| 6                | document change two | sync document `v6` to the server                | `textDocument/didChange`  |

_Client Capability_:
See general synchronization [client capabilities](#textDocument_synchronization_cc).

_Server Capability_:
See general synchronization [server capabilities](#textDocument_synchronization_sc).

_Registration Options_: `TextDocumentChangeRegistrationOptions` defined as follows:

<div class="anchorHolder"><a href="#textDocumentChangeRegistrationOptions" name="textDocumentChangeRegistrationOptions" class="linkableAnchor"></a></div>

```typescript
/**
 * Describe options to be used when registering for text document change events.
 */
export interface TextDocumentChangeRegistrationOptions
	extends TextDocumentRegistrationOptions {
	/**
	 * How documents are synced to the server. See TextDocumentSyncKind.Full
	 * and TextDocumentSyncKind.Incremental.
	 */
	syncKind: TextDocumentSyncKind;
}
```

_Notification_:
* method: `textDocument/didChange`
* params: `DidChangeTextDocumentParams` defined as follows:

<div class="anchorHolder"><a href="#didChangeTextDocumentParams" name="didChangeTextDocumentParams" class="linkableAnchor"></a></div>

```typescript
interface DidChangeTextDocumentParams {
	/**
	 * The document that did change. The version number points
	 * to the version after all provided content changes have
	 * been applied.
	 */
	textDocument: VersionedTextDocumentIdentifier;

	/**
	 * The actual content changes. The content changes describe single state
	 * changes to the document. So if there are two content changes c1 (at
	 * array index 0) and c2 (at array index 1) for a document in state S then
	 * c1 moves the document from S to S' and c2 from S' to S''. So c1 is
	 * computed on the state S and c2 is computed on the state S'.
	 *
	 * To mirror the content of a document using change events use the following
	 * approach:
	 * - start with the same initial content
	 * - apply the 'textDocument/didChange' notifications in the order you
	 *   receive them.
	 * - apply the `TextDocumentContentChangeEvent`s in a single notification
	 *   in the order you receive them.
	 */
	contentChanges: TextDocumentContentChangeEvent[];
}
```

<div class="anchorHolder"><a href="#textDocumentContentChangeEvent" name="textDocumentContentChangeEvent" class="linkableAnchor"></a></div>

```typescript
/**
 * An event describing a change to a text document. If only a text is provided
 * it is considered to be the full content of the document.
 */
export type TextDocumentContentChangeEvent = {
	/**
	 * The range of the document that changed.
	 */
	range: Range;

	/**
	 * The optional length of the range that got replaced.
	 *
	 * @deprecated use range instead.
	 */
	rangeLength?: uinteger;

	/**
	 * The new text for the provided range.
	 */
	text: string;
} | {
	/**
	 * The new text of the whole document.
	 */
	text: string;
};
```



#### <a href="#textDocument_willSave" name="textDocument_willSave" class="anchor">WillSaveTextDocument Notification (:arrow_right:)</a>

The document will save notification is sent from the client to the server before the document is actually saved. If a server has registered for open / close events clients should ensure that the document is open before a `willSave` notification is sent since clients can't change the content of a file without ownership transferal.

_Client Capability_:
* property name (optional): `textDocument.synchronization.willSave`
* property type: `boolean`

The capability indicates that the client supports `textDocument/willSave` notifications.

_Server Capability_:
* property name (optional): `textDocumentSync.willSave`
* property type: `boolean`

The capability indicates that the server is interested in `textDocument/willSave` notifications.

_Registration Options_: `TextDocumentRegistrationOptions`

_Notification_:
* method: 'textDocument/willSave'
* params: `WillSaveTextDocumentParams` defined as follows:

<div class="anchorHolder"><a href="#willSaveTextDocumentParams" name="willSaveTextDocumentParams" class="linkableAnchor"></a></div>

```typescript
/**
 * The parameters send in a will save text document notification.
 */
export interface WillSaveTextDocumentParams {
	/**
	 * The document that will be saved.
	 */
	textDocument: TextDocumentIdentifier;

	/**
	 * The 'TextDocumentSaveReason'.
	 */
	reason: TextDocumentSaveReason;
}
```

<div class="anchorHolder"><a href="#textDocumentSaveReason" name="textDocumentSaveReason" class="linkableAnchor"></a></div>

```typescript
/**
 * Represents reasons why a text document is saved.
 */
export namespace TextDocumentSaveReason {

	/**
	 * Manually triggered, e.g. by the user pressing save, by starting
	 * debugging, or by an API call.
	 */
	export const Manual = 1;

	/**
	 * Automatic after a delay.
	 */
	export const AfterDelay = 2;

	/**
	 * When the editor lost focus.
	 */
	export const FocusOut = 3;
}

export type TextDocumentSaveReason = 1 | 2 | 3;
```



#### <a href="#textDocument_willSaveWaitUntil" name="textDocument_willSaveWaitUntil" class="anchor">WillSaveWaitUntilTextDocument Request (:leftwards_arrow_with_hook:)</a>

The document will save request is sent from the client to the server before the document is actually saved. The request can return an array of TextEdits which will be applied to the text document before it is saved. Please note that clients might drop results if computing the text edits took too long or if a server constantly fails on this request. This is done to keep the save fast and reliable.  If a server has registered for open / close events clients should ensure that the document is open before a `willSaveWaitUntil` notification is sent since clients can't change the content of a file without ownership transferal.

_Client Capability_:
* property name (optional): `textDocument.synchronization.willSaveWaitUntil`
* property type: `boolean`

The capability indicates that the client supports `textDocument/willSaveWaitUntil` requests.

_Server Capability_:
* property name (optional): `textDocumentSync.willSaveWaitUntil`
* property type: `boolean`

The capability indicates that the server is interested in `textDocument/willSaveWaitUntil` requests.

_Registration Options_: `TextDocumentRegistrationOptions`

_Request_:
* method: `textDocument/willSaveWaitUntil`
* params: `WillSaveTextDocumentParams`

_Response_:
* result: [`TextEdit[]`](#textEdit) \| `null`
* error: code and message set in case an exception happens during the `textDocument/willSaveWaitUntil` request.



#### <a href="#textDocument_didSave" name="textDocument_didSave" class="anchor">DidSaveTextDocument Notification (:arrow_right:)</a>

The document save notification is sent from the client to the server when the document was saved in the client.

_Client Capability_:
* property name (optional): `textDocument.synchronization.didSave`
* property type: `boolean`

The capability indicates that the client supports `textDocument/didSave` notifications.

_Server Capability_:
* property name (optional): `textDocumentSync.save`
* property type: `boolean | SaveOptions` where `SaveOptions` is defined as follows:

<div class="anchorHolder"><a href="#saveOptions" name="saveOptions" class="linkableAnchor"></a></div>

```typescript
export interface SaveOptions {
	/**
	 * The client is supposed to include the content on save.
	 */
	includeText?: boolean;
}
```

The capability indicates that the server is interested in `textDocument/didSave` notifications.

_Registration Options_: `TextDocumentSaveRegistrationOptions` defined as follows:

<div class="anchorHolder"><a href="#textDocumentSaveRegistrationOptions" name="textDocumentSaveRegistrationOptions" class="linkableAnchor"></a></div>

```typescript
export interface TextDocumentSaveRegistrationOptions
	extends TextDocumentRegistrationOptions {
	/**
	 * The client is supposed to include the content on save.
	 */
	includeText?: boolean;
}
```

_Notification_:
* method: `textDocument/didSave`
* params: `DidSaveTextDocumentParams` defined as follows:

<div class="anchorHolder"><a href="#didSaveTextDocumentParams" name="didSaveTextDocumentParams" class="linkableAnchor"></a></div>

```typescript
interface DidSaveTextDocumentParams {
	/**
	 * The document that was saved.
	 */
	textDocument: TextDocumentIdentifier;

	/**
	 * Optional the content when saved. Depends on the includeText value
	 * when the save notification was requested.
	 */
	text?: string;
}
```



#### <a href="#textDocument_didClose" name="textDocument_didClose" class="anchor">DidCloseTextDocument Notification (:arrow_right:)</a>

The document close notification is sent from the client to the server when the document got closed in the client. The document's master now exists where the document's Uri points to (e.g. if the document's Uri is a file Uri the master now exists on disk). As with the open notification the close notification is about managing the document's content. Receiving a close notification doesn't mean that the document was open in an editor before. A close notification requires a previous open notification to be sent. Note that a server's ability to fulfill requests is independent of whether a text document is open or closed.

_Client Capability_:
See general synchronization [client capabilities](#textDocument_synchronization_cc).

_Server Capability_:
See general synchronization [server capabilities](#textDocument_synchronization_sc).

_Registration Options_: `TextDocumentRegistrationOptions`

_Notification_:
* method: `textDocument/didClose`
* params: `DidCloseTextDocumentParams` defined as follows:

<div class="anchorHolder"><a href="#didCloseTextDocumentParams" name="didCloseTextDocumentParams" class="linkableAnchor"></a></div>

```typescript
interface DidCloseTextDocumentParams {
	/**
	 * The document that was closed.
	 */
	textDocument: TextDocumentIdentifier;
}
```



#### <a href="#textDocument_didRename" name="textDocument_didRename" class="anchor">Renaming a document</a>

Document renames should be signaled to a server sending a document close notification with the document's old name followed by an open notification using the document's new name. Major reason is that besides the name other attributes can change as well like the language that is associated with the document. In addition the new document could not be of interest for the server anymore.

Servers can participate in a document rename by subscribing for the [`workspace/didRenameFiles`](#workspace_didRenameFiles) notification or the [`workspace/willRenameFiles`](#workspace_willRenameFiles) request.



The final structure of the `TextDocumentSyncClientCapabilities` and the `TextDocumentSyncOptions` server options look like this

<div class="anchorHolder"><a href="#textDocumentSyncClientCapabilities" name="textDocumentSyncClientCapabilities" class="linkableAnchor"></a></div>

```typescript
export interface TextDocumentSyncClientCapabilities {
	/**
	 * Whether text document synchronization supports dynamic registration.
	 */
	dynamicRegistration?: boolean;

	/**
	 * The client supports sending will save notifications.
	 */
	willSave?: boolean;

	/**
	 * The client supports sending a will save request and
	 * waits for a response providing text edits which will
	 * be applied to the document before it is saved.
	 */
	willSaveWaitUntil?: boolean;

	/**
	 * The client supports did save notifications.
	 */
	didSave?: boolean;
}
```

<div class="anchorHolder"><a href="#textDocumentSyncOptions" name="textDocumentSyncOptions" class="linkableAnchor"></a></div>

```typescript
export interface TextDocumentSyncOptions {
	/**
	 * Open and close notifications are sent to the server. If omitted open
	 * close notification should not be sent.
	 */
	openClose?: boolean;
	/**
	 * Change notifications are sent to the server. See
	 * TextDocumentSyncKind.None, TextDocumentSyncKind.Full and
	 * TextDocumentSyncKind.Incremental. If omitted it defaults to
	 * TextDocumentSyncKind.None.
	 */
	change?: TextDocumentSyncKind;
	/**
	 * If present will save notifications are sent to the server. If omitted
	 * the notification should not be sent.
	 */
	willSave?: boolean;
	/**
	 * If present will save wait until requests are sent to the server. If
	 * omitted the request should not be sent.
	 */
	willSaveWaitUntil?: boolean;
	/**
	 * If present save notifications are sent to the server. If omitted the
	 * notification should not be sent.
	 */
	save?: boolean | SaveOptions;
}
```



<!-- ==================== The definition family ==================== -->

#### <a href="#textDocument_definition" name="textDocument_definition" class="anchor">Goto Definition Request (:leftwards_arrow_with_hook:)</a>

The go to definition request is sent from the client to the server to resolve the definition location of a symbol at a given text document position.

The result type [`LocationLink`](#locationLink)[] got introduced with version 3.14.0 and depends on the corresponding client capability `textDocument.definition.linkSupport`.

_Client Capability_:
* property name (optional): `textDocument.definition`
* property type: `DefinitionClientCapabilities` defined as follows:

<div class="anchorHolder"><a href="#definitionClientCapabilities" name="definitionClientCapabilities" class="linkableAnchor"></a></div>

```typescript
export interface DefinitionClientCapabilities {
	/**
	 * Whether definition supports dynamic registration.
	 */
	dynamicRegistration?: boolean;

	/**
	 * The client supports additional metadata in the form of definition links.
	 *
	 * @since 3.14.0
	 */
	linkSupport?: boolean;
}
```

_Server Capability_:
* property name (optional): `definitionProvider`
* property type: `boolean | DefinitionOptions` where `DefinitionOptions` is defined as follows:

<div class="anchorHolder"><a href="#definitionOptions" name="definitionOptions" class="linkableAnchor"></a></div>

```typescript
export interface DefinitionOptions extends WorkDoneProgressOptions {
}
```

_Registration Options_: `DefinitionRegistrationOptions` defined as follows:

<div class="anchorHolder"><a href="#definitionRegistrationOptions" name="definitionRegistrationOptions" class="linkableAnchor"></a></div>

```typescript
export interface DefinitionRegistrationOptions extends
	TextDocumentRegistrationOptions, DefinitionOptions {
}
```

_Request_:
* method: `textDocument/definition`
* params: `DefinitionParams` defined as follows:

<div class="anchorHolder"><a href="#definitionParams" name="definitionParams" class="linkableAnchor"></a></div>

```typescript
export interface DefinitionParams extends TextDocumentPositionParams,
	WorkDoneProgressParams, PartialResultParams {
}
```

_Response_:
* result: [`Location`](#location) \| [`Location`](#location)[] \| [`LocationLink`](#locationLink)[] \| `null`
* partial result: [`Location`](#location)[] \| [`LocationLink`](#locationLink)[]
* error: code and message set in case an exception happens during the definition request.

#### <a href="#textDocument_declaration" name="textDocument_declaration" class="anchor">Goto Declaration Request (:leftwards_arrow_with_hook:)</a>

> *Since version 3.14.0*

The go to declaration request is sent from the client to the server to resolve the declaration location of a symbol at a given text document position.

The result type [`LocationLink`](#locationLink)[] got introduced with version 3.14.0 and depends on the corresponding client capability `textDocument.declaration.linkSupport`.

_Client Capability_:
* property name (optional): `textDocument.declaration`
* property type: `DeclarationClientCapabilities` defined as follows:

<div class="anchorHolder"><a href="#declarationClientCapabilities" name="declarationClientCapabilities" class="linkableAnchor"></a></div>

```typescript
export interface DeclarationClientCapabilities {
	/**
	 * Whether declaration supports dynamic registration. If this is set to
	 * `true` the client supports the new `DeclarationRegistrationOptions`
	 * return value for the corresponding server capability as well.
	 */
	dynamicRegistration?: boolean;

	/**
	 * The client supports additional metadata in the form of declaration links.
	 */
	linkSupport?: boolean;
}
```

_Server Capability_:
* property name (optional): `declarationProvider`
* property type: `boolean | DeclarationOptions | DeclarationRegistrationOptions` where `DeclarationOptions` is defined as follows:

<div class="anchorHolder"><a href="#declarationOptions" name="declarationOptions" class="linkableAnchor"></a></div>

```typescript
export interface DeclarationOptions extends WorkDoneProgressOptions {
}
```

_Registration Options_: `DeclarationRegistrationOptions` defined as follows:

<div class="anchorHolder"><a href="#declarationRegistrationOptions" name="declarationRegistrationOptions" class="linkableAnchor"></a></div>

```typescript
export interface DeclarationRegistrationOptions extends DeclarationOptions,
	TextDocumentRegistrationOptions, StaticRegistrationOptions {
}
```

_Request_:
* method: `textDocument/declaration`
* params: `DeclarationParams` defined as follows:

<div class="anchorHolder"><a href="#declarationParams" name="declarationParams" class="linkableAnchor"></a></div>

```typescript
export interface DeclarationParams extends TextDocumentPositionParams,
	WorkDoneProgressParams, PartialResultParams {
}
```

_Response_:
* result: [`Location`](#location) \| [`Location`](#location)[] \| [`LocationLink`](#locationLink)[] \|`null`
* partial result: [`Location`](#location)[] \| [`LocationLink`](#locationLink)[]
* error: code and message set in case an exception happens during the declaration request.

#### <a href="#textDocument_typeDefinition" name="textDocument_typeDefinition" class="anchor">Goto Type Definition Request (:leftwards_arrow_with_hook:)</a>

> *Since version 3.6.0*

The go to type definition request is sent from the client to the server to resolve the type definition location of a symbol at a given text document position.

The result type [`LocationLink`](#locationLink)[] got introduced with version 3.14.0 and depends on the corresponding client capability `textDocument.typeDefinition.linkSupport`.

_Client Capability_:
* property name (optional): `textDocument.typeDefinition`
* property type: `TypeDefinitionClientCapabilities` defined as follows:

<div class="anchorHolder"><a href="#typeDefinitionClientCapabilities" name="typeDefinitionClientCapabilities" class="linkableAnchor"></a></div>

```typescript
export interface TypeDefinitionClientCapabilities {
	/**
	 * Whether implementation supports dynamic registration. If this is set to
	 * `true` the client supports the new `TypeDefinitionRegistrationOptions`
	 * return value for the corresponding server capability as well.
	 */
	dynamicRegistration?: boolean;

	/**
	 * The client supports additional metadata in the form of definition links.
	 *
	 * @since 3.14.0
	 */
	linkSupport?: boolean;
}
```

_Server Capability_:
* property name (optional): `typeDefinitionProvider`
* property type: `boolean | TypeDefinitionOptions | TypeDefinitionRegistrationOptions` where `TypeDefinitionOptions` is defined as follows:

<div class="anchorHolder"><a href="#typeDefinitionOptions" name="typeDefinitionOptions" class="linkableAnchor"></a></div>

```typescript
export interface TypeDefinitionOptions extends WorkDoneProgressOptions {
}
```

_Registration Options_: `TypeDefinitionRegistrationOptions` defined as follows:

<div class="anchorHolder"><a href="#typeDefinitionRegistrationOptions" name="typeDefinitionRegistrationOptions" class="linkableAnchor"></a></div>

```typescript
export interface TypeDefinitionRegistrationOptions extends
	TextDocumentRegistrationOptions, TypeDefinitionOptions,
	StaticRegistrationOptions {
}
```

_Request_:
* method: `textDocument/typeDefinition`
* params: `TypeDefinitionParams` defined as follows:

<div class="anchorHolder"><a href="#typeDefinitionParams" name="typeDefinitionParams" class="linkableAnchor"></a></div>

```typescript
export interface TypeDefinitionParams extends TextDocumentPositionParams,
	WorkDoneProgressParams, PartialResultParams {
}
```

_Response_:
* result: [`Location`](#location) \| [`Location`](#location)[] \| [`LocationLink`](#locationLink)[] \| `null`
* partial result: [`Location`](#location)[] \| [`LocationLink`](#locationLink)[]
* error: code and message set in case an exception happens during the definition request.

#### <a href="#textDocument_implementation" name="textDocument_implementation" class="anchor">Goto Implementation Request (:leftwards_arrow_with_hook:)</a>

> *Since version 3.6.0*

The go to implementation request is sent from the client to the server to resolve the implementation location of a symbol at a given text document position.

The result type [`LocationLink`](#locationLink)[] got introduced with version 3.14.0 and depends on the corresponding client capability `textDocument.implementation.linkSupport`.

_Client Capability_:
* property name (optional): `textDocument.implementation`
* property type: `ImplementationClientCapabilities` defined as follows:

<div class="anchorHolder"><a href="#implementationClientCapabilities" name="implementationClientCapabilities" class="linkableAnchor"></a></div>

```typescript
export interface ImplementationClientCapabilities {
	/**
	 * Whether implementation supports dynamic registration. If this is set to
	 * `true` the client supports the new `ImplementationRegistrationOptions`
	 * return value for the corresponding server capability as well.
	 */
	dynamicRegistration?: boolean;

	/**
	 * The client supports additional metadata in the form of definition links.
	 *
	 * @since 3.14.0
	 */
	linkSupport?: boolean;
}
```

_Server Capability_:
* property name (optional): `implementationProvider`
* property type: `boolean | ImplementationOptions | ImplementationRegistrationOptions` where `ImplementationOptions` is defined as follows:

<div class="anchorHolder"><a href="#implementationOptions" name="implementationOptions" class="linkableAnchor"></a></div>

```typescript
export interface ImplementationOptions extends WorkDoneProgressOptions {
}
```

_Registration Options_: `ImplementationRegistrationOptions` defined as follows:

<div class="anchorHolder"><a href="#implementationRegistrationOptions" name="implementationRegistrationOptions" class="linkableAnchor"></a></div>

```typescript
export interface ImplementationRegistrationOptions extends
	TextDocumentRegistrationOptions, ImplementationOptions,
	StaticRegistrationOptions {
}
```

_Request_:
* method: `textDocument/implementation`
* params: `ImplementationParams` defined as follows:

<div class="anchorHolder"><a href="#implementationParams" name="implementationParams" class="linkableAnchor"></a></div>

```typescript
export interface ImplementationParams extends TextDocumentPositionParams,
	WorkDoneProgressParams, PartialResultParams {
}
```

_Response_:
* result: [`Location`](#location) \| [`Location`](#location)[] \| [`LocationLink`](#locationLink)[] \| `null`
* partial result: [`Location`](#location)[] \| [`LocationLink`](#locationLink)[]
* error: code and message set in case an exception happens during the definition request.
