// StickerNest WeChat key dumper — minimal runtime key capture used only when
// the user explicitly launches the one-shot clone. Inject with
// DYLD_INSERT_LIBRARIES into an adhoc-resigned CLONE of WeChat.app (never the
// installed copy), the dumper reads emoticon salts ahead of time, observes the
// SQLCipher key material the clone derives, and writes it to the file named by
// SN_KEY_OUT. fishhook.c/h are bundled unmodified (facebook/fishhook, BSD-3).
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <unistd.h>
#include <fcntl.h>
#include <errno.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <dirent.h>
#include <dlfcn.h>
#include <CommonCrypto/CommonKeyDerivation.h>
#include <CommonCrypto/CommonCryptoError.h>
#include "fishhook.h"

#define MAX_ACCOUNTS 16
#define SALT_LEN 16
#define KEY_HEX_MAX 128

typedef struct {
    char wxid[128];
    unsigned char salt[SALT_LEN];
    unsigned char mac_salt[SALT_LEN];
} account_salt;

static account_salt g_accounts[MAX_ACCOUNTS];
static size_t g_account_count = 0;
static char g_key_out[1024] = {0};
static char g_target_wxid[128] = {0};
static bool g_done = false;

static bool read_file_prefix(const char *path, unsigned char *out, size_t len) {
    int fd = open(path, O_RDONLY);
    if (fd < 0) return false;
    ssize_t got = read(fd, out, len);
    close(fd);
    return got == (ssize_t)len;
}

static void account_add(const char *wxid, const unsigned char *salt) {
    if (g_account_count >= MAX_ACCOUNTS) return;
    account_salt *slot = &g_accounts[g_account_count++];
    snprintf(slot->wxid, sizeof(slot->wxid), "%s", wxid);
    memcpy(slot->salt, salt, SALT_LEN);
    for (int i = 0; i < SALT_LEN; i++) slot->mac_salt[i] = salt[i] ^ 0x3a;
}

static void scan_accounts(void) {
    const char *home = getenv("HOME");
    if (!home) return;
    char root[1024];
    snprintf(root, sizeof(root),
             "%s/Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files", home);
    DIR *dir = opendir(root);
    if (!dir) return;
    struct dirent *entry;
    char path[1536];
    unsigned char salt[SALT_LEN];
    while ((entry = readdir(dir)) != NULL) {
        if (entry->d_name[0] == '.') continue;
        if (strncmp(entry->d_name, "wxid_", 5) != 0) continue;
        snprintf(path, sizeof(path),
                 "%s/%s/db_storage/emoticon/emoticon.db", root, entry->d_name);
        if (read_file_prefix(path, salt, SALT_LEN)) {
            account_add(entry->d_name, salt);
        }
    }
    closedir(dir);
}

static bool salt_matches(const unsigned char *salt) {
    for (size_t i = 0; i < g_account_count; i++) {
        if (memcmp(g_accounts[i].salt, salt, SALT_LEN) == 0) return true;
        if (memcmp(g_accounts[i].mac_salt, salt, SALT_LEN) == 0) return true;
    }
    return false;
}

static void write_key_hex(const unsigned char *bytes, size_t n) {
    if (g_done || !g_key_out[0] || n == 0 || n > (KEY_HEX_MAX / 2)) return;
    char hex[KEY_HEX_MAX + 1];
    for (size_t i = 0; i < n; i++) snprintf(hex + i * 2, 3, "%02x", bytes[i]);
    hex[n * 2] = '\0';
    int fd = open(g_key_out, O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (fd < 0) return;
    (void)!write(fd, hex, strlen(hex));
    close(fd);
    g_done = true;
}

// ---- CCKeyDerivationPBKDF hook: WeChat 4.1+ derives the key via PBKDF2 with
// the emoticon.db salt as the salt argument. ----
static int (*orig_CCKeyDerivationPBKDF)(CCPBKDFAlgorithm, const char *, size_t,
                                        const uint8_t *, size_t, CCPseudoRandomAlgorithm,
                                        uint, uint8_t *, size_t);
static int hook_CCKeyDerivationPBKDF(CCPBKDFAlgorithm algorithm, const char *password,
                                     size_t passwordLen, const uint8_t *salt, size_t saltLen,
                                     CCPseudoRandomAlgorithm prf, uint rounds,
                                     uint8_t *derivedKey, size_t derivedKeyLen) {
    int result = orig_CCKeyDerivationPBKDF(algorithm, password, passwordLen, salt,
                                           saltLen, prf, rounds, derivedKey, derivedKeyLen);
    if (!g_done && result == kCCSuccess && saltLen == SALT_LEN && salt_matches(salt)
        && password && passwordLen > 0 && passwordLen <= (KEY_HEX_MAX / 2)
        && strlen(password) == passwordLen) {
        // passphrase form: capture the ASCII passphrase, downstream derives it.
        write_key_hex((const unsigned char *)password, passwordLen);
    }
    return result;
}

// ---- sqlite3_key hooks: if the clone passes the key directly, capture it. ----
static int (*orig_sqlite3_key)(void *db, const void *pKey, int nKey);
static int hook_sqlite3_key(void *db, const void *pKey, int nKey) {
    int result = orig_sqlite3_key(db, pKey, nKey);
    if (!g_done && pKey && nKey > 0 && nKey <= (KEY_HEX_MAX / 2)) {
        write_key_hex(pKey, (size_t)nKey);
    }
    return result;
}

static int (*orig_sqlite3_key_v2)(void *db, const char *zDbName, const void *pKey, int nKey);
static int hook_sqlite3_key_v2(void *db, const char *zDbName, const void *pKey, int nKey) {
    int result = orig_sqlite3_key_v2(db, zDbName, pKey, nKey);
    if (!g_done && pKey && nKey > 0 && nKey <= (KEY_HEX_MAX / 2)) {
        write_key_hex(pKey, (size_t)nKey);
    }
    return result;
}

__attribute__((constructor))
static void sn_dumper_init(void) {
    const char *out = getenv("SN_KEY_OUT");
    if (out) snprintf(g_key_out, sizeof(g_key_out), "%s", out);
    const char *target = getenv("SN_TARGET_WXID");
    if (target) snprintf(g_target_wxid, sizeof(g_target_wxid), "%s", target);
    scan_accounts();

    struct rebinding bindings[] = {
        {"CCKeyDerivationPBKDF", hook_CCKeyDerivationPBKDF, (void **)&orig_CCKeyDerivationPBKDF},
        {"sqlite3_key", hook_sqlite3_key, (void **)&orig_sqlite3_key},
        {"sqlite3_key_v2", hook_sqlite3_key_v2, (void **)&orig_sqlite3_key_v2},
    };
    int rc = rebind_symbols(bindings, 3);
    if (rc != 0 && g_key_out[0]) {
        (void)!write(open(g_key_out, O_WRONLY | O_CREAT | O_TRUNC, 0600), "", 0);
        int fd = open(g_key_out, O_WRONLY | O_CREAT | O_TRUNC, 0600);
        if (fd >= 0) {
            (void)!write(fd, "REBIND_FAILED", 13);
            close(fd);
        }
    }
}
