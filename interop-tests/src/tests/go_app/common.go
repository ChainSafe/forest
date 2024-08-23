package main

const ListenAddr = "/ip4/127.0.0.1/tcp/0"

func checkError(err error) {
	if err != nil {
		panic(err)
	}
}
