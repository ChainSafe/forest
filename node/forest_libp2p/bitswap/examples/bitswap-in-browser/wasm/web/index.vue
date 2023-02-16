<template>
    <div class="container px-4 prose">
        <h2>Bitswap in browser (WebAssembly)</h2>
    </div>
    <p>
        Peer address: <br />
        <input v-model="addr" class="form-input w-full"> <br />
        Block CID <br />
        <input v-model="cid" class="form-input w-full"> <br />
        <button v-if="wasmLoaded"
            class="bg-sky-500 hover:bg-sky-700 px-5 py-2 text-sm leading-5 rounded-full font-semibold text-white my-2"
            @click="bitswap_get">
            Get block via bitswap
        </button>
        &nbsp;&nbsp;
        <button v-if="wasmLoaded"
            class="bg-sky-500 hover:bg-sky-700 px-5 py-2 text-sm leading-5 rounded-full font-semibold text-white my-2"
            @click="random_cid">Generate random CID</button>
    </p>
    <p>
    <p v-for="(value, key) of responses">
        {{ key }} => {{ value }}
    </p>
</p>
</template>

<script lang="ts">
import { initWasm } from "../utils";
import { init_logger, connect, Connection, random_cid } from "../pkg/wasm";
import { EventEmitter } from "events";

export default {
    data() {
        return {
            wasmLoaded: false,
            addr: "/ip4/127.0.0.1/tcp/0/ws/[peer_id]",
            cid: "",
            eventEmitter: new EventEmitter(),
            connection: null,
            responses: {}
        };
    },
    created() {
        this.loadWasm();
    },
    methods: {
        async loadWasm() {
            await initWasm();
            this.wasmLoaded = true;
            init_logger();
            this.random_cid();
            const self = this;
            this.eventEmitter.on('bitswap', e => {
                const { cid, response } = JSON.parse(e);
                console.log(cid, response);
                self.responses[cid] = response;
            });
        },
        wasmStatus() {
            return this.wasmLoaded ? "loaded" : "loading";
        },
        async bitswap_get() {
            if (!this.connection) {
                await this.connect()
            }
            let conn = this.connection as Connection;
            this.cid = this.cid.trim();
            this.responses[this.cid] = "...";
            conn.bitswap_get(this.cid);
        },
        random_cid() {
            this.cid = random_cid();
        },
        async connect() {
            if (!this.wasmLoaded) {
                return;
            }
            console.log(`[JS] Connecting to ${this.addr}`);
            try {
                this.addr = this.addr.trim();
                const connection = await connect(this.addr, this.eventEmitter);
                let oldConn = this.connection as Connection;
                if (oldConn) {
                    oldConn.free();
                }
                this.connection = connection;
                console.log(`[JS] Connected to ${this.addr}`);
                connection.run()
            } catch (e) {
                alert(e);
            }
        },
    },
};
</script>
<style lang="scss" scoped></style>