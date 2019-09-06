#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C"
{
#endif

    typedef struct _webview *webview;

    typedef struct webview_options
    {
        size_t initial_width,
            initial_height,
            minimum_width,
            minimum_height;
        bool borderless,
            debug;
        void *data;
        void (*message)(void *data, const char *message);
        void (*closed)(void *data);
    } webview_options;

    void webview_start(void (*func)(void));

    void webview_dispatch(void *data, void (*func)(void *data));
    void webview_exit(void);

    webview webview_new(webview_options opts);

    void webview_eval(webview self, const char *js);
    void webview_load(webview self, const char *html);
    void webview_title(webview self, const char *title);

    void webview_focus(webview self);
    void webview_close(webview self);

#ifdef __cplusplus
}
#endif