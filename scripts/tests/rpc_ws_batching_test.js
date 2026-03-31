// `WebSocket` is available since node@21: <https://nodejs.org/en/learn/getting-started/websocket#introduction>
const ws = new WebSocket('ws://localhost:2345/rpc/v1');

ws.addEventListener('open', () => {
    ws.send(JSON.stringify([{ "jsonrpc": "2.0", "method": "Filecoin.Version", "id": 1 }, { "jsonrpc": "2.0", "method": "Filecoin.Session", "id": 2 }]));
});

ws.addEventListener('message', (e) => {
    var data = JSON.parse(e.data);
    console.log('response:\n', data);
    ws.close();
    if (data.length === 2 && data[0]['result']['BlockDelay'] === 30) {
        console.log('success');
        process.exit(0);
    } else {
        console.log('Error: bad response');
        process.exit(1);
    }
});

setTimeout(() => {
    console.log('Error: failed to get response in 5s');
    ws.close();
    process.exit(1);
}, 5000);
