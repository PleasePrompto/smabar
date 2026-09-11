#include <libsoup/soup.h>
#include <webkit2/webkit-web-extension.h>

#define APP_IDENTITY "https://dev.smabar.desktop/"
#define ORIGIN_MESSAGE "smabar-provider-origin"

static gchar *wrapper_origin = NULL;

static gboolean set_origin(GVariant *parameters) {
  if (parameters == NULL ||
      !g_variant_is_of_type(parameters, G_VARIANT_TYPE_STRING)) {
    return FALSE;
  }
  g_free(wrapper_origin);
  wrapper_origin = g_variant_dup_string(parameters, NULL);
  return TRUE;
}

static gboolean identify_provider_request(WebKitWebPage *page,
                                          WebKitURIRequest *request,
                                          WebKitURIResponse *redirected_response,
                                          gpointer user_data) {
  const gchar *uri = webkit_uri_request_get_uri(request);
  SoupMessageHeaders *headers = webkit_uri_request_get_http_headers(request);
  const gchar *referer =
      headers == NULL ? NULL : soup_message_headers_get_one(headers, "Referer");

  (void)page;
  (void)redirected_response;
  (void)user_data;
  if (wrapper_origin != NULL && uri != NULL &&
      g_str_has_prefix(uri, "https://") &&
      g_strcmp0(referer, wrapper_origin) == 0) {
    soup_message_headers_replace(headers, "Referer", APP_IDENTITY);
  }
  return FALSE;
}

static gboolean receive_origin(WebKitWebExtension *extension,
                               WebKitUserMessage *message,
                               gpointer user_data) {
  GVariant *parameters = webkit_user_message_get_parameters(message);

  (void)extension;
  (void)user_data;
  if (g_strcmp0(webkit_user_message_get_name(message), ORIGIN_MESSAGE) != 0) {
    return FALSE;
  }
  return set_origin(parameters);
}

static void watch_page(WebKitWebExtension *extension, WebKitWebPage *page,
                       gpointer user_data) {
  (void)extension;
  (void)user_data;
  g_signal_connect(page, "send-request", G_CALLBACK(identify_provider_request),
                   NULL);
}

G_MODULE_EXPORT void
webkit_web_extension_initialize(WebKitWebExtension *extension) {
  g_signal_connect(extension, "page-created", G_CALLBACK(watch_page), NULL);
  g_signal_connect(extension, "user-message-received",
                   G_CALLBACK(receive_origin), NULL);
}

G_MODULE_EXPORT void webkit_web_extension_initialize_with_user_data(
    WebKitWebExtension *extension, GVariant *user_data) {
  set_origin(user_data);
  webkit_web_extension_initialize(extension);
}
