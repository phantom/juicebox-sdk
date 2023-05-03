#ifndef LOAM_FFI_H_
#define LOAM_FFI_H_

/* This file was automatically generated by cbindgen */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef enum {
  LoamDeleteErrorInvalidAuth = 0,
  LoamDeleteErrorAssertion = 1,
  LoamDeleteErrorTransient = 2,
} LoamDeleteError;

typedef enum {
  LoamHttpRequestMethodGet = 0,
  LoamHttpRequestMethodPut,
  LoamHttpRequestMethodPost,
  LoamHttpRequestMethodDelete,
} LoamHttpRequestMethod;

typedef enum {
  /**
   * A tuned hash, secure for use on modern devices as of 2019 with low-entropy PINs.
   */
  LoamPinHashingModeStandard2019 = 0,
  /**
   * A fast hash used for testing. Do not use in production.
   */
  LoamPinHashingModeFastInsecure = 1,
} LoamPinHashingMode;

typedef enum {
  LoamRecoverErrorReasonInvalidPin = 0,
  LoamRecoverErrorReasonNotRegistered = 1,
  LoamRecoverErrorReasonInvalidAuth = 2,
  LoamRecoverErrorReasonAssertion = 3,
  LoamRecoverErrorReasonTransient = 4,
} LoamRecoverErrorReason;

typedef enum {
  LoamRegisterErrorInvalidAuth = 0,
  LoamRegisterErrorAssertion = 1,
  LoamRegisterErrorTransient = 2,
} LoamRegisterError;

typedef struct LoamClient LoamClient;

typedef struct LoamHttpClient LoamHttpClient;

typedef struct {
  const uint8_t *data;
  size_t length;
} LoamUnmanagedDataArray;

typedef struct {
  uint8_t id[16];
  const char *address;
  LoamUnmanagedDataArray public_key;
} LoamRealm;

typedef struct {
  const LoamRealm *data;
  size_t length;
} LoamUnmanagedRealmArray;

typedef struct {
  LoamUnmanagedRealmArray realms;
  uint8_t register_threshold;
  uint8_t recover_threshold;
  LoamPinHashingMode pin_hashing_mode;
} LoamConfiguration;

typedef struct {
  const LoamConfiguration *data;
  size_t length;
} LoamUnmanagedConfigurationArray;

typedef struct {
  const char *name;
  const char *value;
} LoamHttpHeader;

typedef struct {
  const LoamHttpHeader *data;
  size_t length;
} LoamUnmanagedHttpHeaderArray;

typedef struct {
  uint8_t id[16];
  LoamHttpRequestMethod method;
  const char *url;
  LoamUnmanagedHttpHeaderArray headers;
  LoamUnmanagedDataArray body;
} LoamHttpRequest;

typedef struct {
  uint8_t id[16];
  uint16_t status_code;
  LoamUnmanagedHttpHeaderArray headers;
  LoamUnmanagedDataArray body;
} LoamHttpResponse;

typedef void (*LoamHttpResponseFn)(LoamHttpClient *context, const LoamHttpResponse *response);

typedef void (*LoamHttpSendFn)(const LoamHttpClient *context, const LoamHttpRequest *request, LoamHttpResponseFn callback);

typedef struct {
  LoamRecoverErrorReason reason;
  /**
   * If non-NULL, the number of guesses remaining after an Unsuccessful attempt.
   */
  const uint16_t *guesses_remaining;
} LoamRecoverError;

/**
 * Constructs a new opaque `LoamClient`.
 *
 * # Arguments
 *
 * * `configuration` – Represents the current configuration. The configuration
 * provided must include at least one `LoamRealm`.
 * * `previous_configurations` – Represents any other configurations you have
 * previously registered with that you may not yet have migrated the data from.
 * During `loam_client_recover`, they will be tried if the current user has not yet
 * registered on the current configuration. These should be ordered from most recently
 * to least recently used.
 * * `auth_token` – Represents the authority to act as a particular user
 * and should be valid for the lifetime of the `LoamClient`.
 * * `http_send` – A function pointer `http_send` that will be called when the client
 * wishes to make a network request. The appropriate request should be executed by you,
 * and the the response provided to the response function pointer. This send
 * should be performed asynchronously. `http_send` should not block on
 * performing the request, and the response should be returned to the
 * `response` function pointer argument when the asynchronous work has
 * completed. The request parameter is only valid for the lifetime of the
 * `http_send` function and should not be accessed after returning from the
 * function.
 */
LoamClient *loam_client_create(LoamConfiguration configuration,
                               LoamUnmanagedConfigurationArray previous_configurations,
                               const char *auth_token,
                               LoamHttpSendFn http_send);

void loam_client_destroy(LoamClient *client);

/**
 * Stores a new PIN-protected secret on the configured realms.
 *
 * # Note
 *
 * The provided secret must have a maximum length of 128-bytes.
 */
void loam_client_register(LoamClient *client,
                          const void *context,
                          LoamUnmanagedDataArray pin,
                          LoamUnmanagedDataArray secret,
                          uint16_t num_guesses,
                          void (*response)(const void *context, const LoamRegisterError *error));

/**
 * Retrieves a PIN-protected secret from the configured realms, or falls
 * back to the previous realms if the current realms do not have a secret
 * registered.
 */
void loam_client_recover(LoamClient *client,
                         const void *context,
                         LoamUnmanagedDataArray pin,
                         void (*response)(const void *context, LoamUnmanagedDataArray secret, const LoamRecoverError *error));

/**
 * Deletes the registered secret for this user, if any.
 */
void loam_client_delete(LoamClient *client,
                        const void *context,
                        void (*response)(const void *context, const LoamDeleteError *error));

#endif /* LOAM_FFI_H_ */
