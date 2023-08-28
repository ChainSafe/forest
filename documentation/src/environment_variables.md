# Environment variables

Besides CLI options and the configuration values in the configuration file,
there are some environment variables that control the behaviour of a `forest`
process.

| Environment variable       | Value     | Default | Description                                              |
| -------------------------- | --------- | ------- | -------------------------------------------------------- |
| FOREST_KEYSTORE_PHRASE_ENV | any text  | empty   | The passphrase for the encrypted keystore                |
| FOREST_CAR_LOADER_FILE_IO  | 1 or true | false   | Load CAR files with `RandomAccessFile` instead of `Mmap` |
