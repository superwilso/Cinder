/* Hand-written for Cinder's static ARM build (wifi/build-userland.sh): AirPlay 1, OpenSSL,
 * tinysvcmdns, the stdout and pipe outputs, Hammerton's ALAC decoder. */
#define PACKAGE_VERSION "5.5.2"
#define CONFIG_OPENSSL 1
#define CONFIG_TINYSVCMDNS 1
#define CONFIG_STDOUT 1
#define CONFIG_PIPE 1
/* The sender's song and cover through a FIFO (-M -g --metadata-pipename): Now Playing shows them. */
#define CONFIG_METADATA 1
#define CONFIG_METADATA_PIPE 1
#define CONFIG_HAMMERTON 1
