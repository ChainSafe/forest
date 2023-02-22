
# Forest JavaScript Console

Forest console stands as an alternative to tools like [Curl](https://github.com/curl/curl) or other subcommands found in `forest-cli` for interacting with the Filecoin JSON-RPC API.

## Starting the console

`forest-cli attach` can be used to open a Javascript console connected to your Forest node.

Like for some other `forest-cli` subcommands you will need to pass an admin `--token` given what endpoints you will call.

For a description of different options please refer to the developer documentation [CLI page](https://github.com/ChainSafe/forest/blob/main/documentation/developer_documentation/CLI.md#cli).

## Interactive Use

First start Forest node inside another terminal:

```bash
forest --chain calibnet
```

To attach to your Forest node, run `forest-cli` with the `attach` subcommand:

```bash
forest-cli --token <TOKEN> attach 
```

You should now see a prompt and be able to interact:

```                                          
Welcome to the Forest Javascript console!

To exit, press ctrl-d or type :quit
> console.log("Forest running on " + chainGetName())
Forest running on calibnet
```

You can directly call JSON-RPC API endpoints that are bound to the console.

For example, `Filecoin.ChainGetName` is bound to the global `chainGetName` function.

### Tips

- The console history is saved in your `~/.forest_history` after exiting.
- Use `:clear` to erase *current* session commands.
- Use `_BOA_VERSION` to get engine version

## Non-interactive Use

### Exec Mode

It is also possible to execute commands non-interactively by passing `--exec` flag and a JavaScript snippet to `forest-cli attach`. The result is displayed directly in the terminal rather than in the interactive console.

For example, to display the current epoch:

```bash
forest-cli attach --exec "syncStatus().ActiveSyncs[0].Epoch"
```

Or print wallet default address:

```bash
forest-cli attach --exec "console.log(walletDefaultAddress())"
```

## Builtins

### Helpers

Forest console comes with a number of helper functions that make interacting with Filecoin API easy:
 - `showPeers()`
 - `getPeer(peerID)`
 - `disconnectPeers(count)`
 - `isPeerConnected(peerID)`
 - `showWallet()`
 - `showSyncStatus()`
 - `sendFIL(to, attoAmount)`

### Modules

CommonJS modules is the way to package JavaScript code for Forest console. You can import modules using the `require` function:

```bash
forest-cli attach --exec "const Math = require('calc'); console.log(Math.add(39,3))"
```

where `calc.js` is:

```javascript
module.exports = {
    add: function (a, b) {
      return a + b;
    },
    multiply: function (a, b) {
      return a * b;
    },
};
```

By default modules will be loaded from the current directory. Use `--jspath` flag to indicate another path.

## Limitations

Forest's console is built using [Boa Javascript engine](https://github.com/boa-dev/boa). It does support promises or `async` functions, but keep in mind that Boa is not fully compatible with ECMAScript yet.

Not every endpoint from the Filecoin API has been bound to the console. Please [create an issue](https://github.com/ChainSafe/forest/issues) if you need one that is not available.
