# chainHead_unstable_storage

**Parameters**:

- `followSubscriptionId`: An opaque string that was returned by `chainHead_unstable_follow`.
- `hash`: String containing an hexadecimal-encoded hash of the header of the block whose storage to fetch.
- `key`: String containing the hexadecimal-encoded key to fetch in the storage.
- `childKey`: `null` for main storage look-ups, or a string containing the hexadecimal-encoded key of the trie key of the trie that `key` refers to. **TODO**: I don't know enough about child tries to design this properly
- `type`: String that must be equal to one of: `value`, `hash`, or `size`.
- `networkConfig` (optional): Object containing the configuration of the networking part of the function. See [here](./introduction.md) for details. Ignored if the JSON-RPC server doesn't need to perform a network request. Sensible defaults are used if not provided.

**Return value**: String containing an opaque value representing the operation.

The JSON-RPC server must start obtaining the value of the entry with the given `key` (and possibly `childKey`) from the storage.

The operation will continue even if the given block is unpinned while it is in progress.

This function should be seen as a complement to `chainHead_unstable_follow`, allowing the JSON-RPC client to retrieve more information about a block that has been reported. Use `archive_unstable_storage` if instead you want to retrieve the storage of an arbitrary block.

For optimization purposes, the JSON-RPC server is allowed to wait a little bit (e.g. up to 100ms) before starting to try fulfill the storage request, in order to batch multiple storage requests together.

## Notifications format

This function will later generate notifications in the following format:

```json
{
    "jsonrpc": "2.0",
    "method": "chainHead_unstable_storageEvent",
    "params": {
        "subscriptionId": "...",
        "result": ...
    }
}
```

Where `subscriptionId` is equal to the value returned by this function, and `result` can be one of:

### done

```json
{
    "event": "done",
    "value": "0x0000000..."
}
```

The `done` event indicates that everything went well. The `value` field contains the requested value.

Where `value` is:

- If `type` was `value`, either `null` if the storage doesn't contain a value at the given key, or a string containing the hexadecimal-encoded value of the storage entry.
- If `type` was `hash`, either `null` if the storage doesn't contain a value at the given key, or a string containing the hexadecimal-encoded hash of the value of the storage item. The hashing algorithm is the same as the one used by the trie of the chain.
- If `type` was `size`, either `null` if the storage doesn't contain a value at the given key, or a string containing the number of bytes of the storage entry. Note that a string is used rather than a number in order to prevent JavaScript clients from accidentally rounding the value.

No more event will be generated with this `subscriptionId`.

### inaccessible

```json
{
    "event": "inaccessible"
}
```

The `inaccessible` event indicates that the storage value has failed to be retrieved from the network.

No more event will be generated with this `subscriptionId`.

### disjoint

```json
{
    "event": "disjoint"
}
```

The `disjoint` event indicates that the provided `followSubscriptionId` is invalid or stale.

No more event will be generated with this `subscriptionId`.

## Possible errors

- A JSON-RPC error is generated if `type` isn't one of the allowed values (similarly to a missing parameter or an invalid parameter type).
- If the networking part of the behaviour fails, then a `{"event": "inaccessible"}` notification is generated (as explained above).
- If the `followSubscriptionId` is invalid or stale, then a `{"event": "disjoint"}` notification is generated (as explained above).
- A JSON-RPC error is generated if the block hash passed as parameter doesn't correspond to any block that has been reported by `chainHead_unstable_follow`.
- A JSON-RPC error is generated if the `followSubscriptionId` is valid but the block hash passed as parameter has already been unpinned.
