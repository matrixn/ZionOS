#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/time.h>
int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "--serve") == 0) {
        const char *directory = getenv("ZPK_RUNTIME_DIR");
        const char *data = getenv("ZPK_DATA_DIR");
        char state[512]; unsigned long count = 0;
        if (!data || snprintf(state, sizeof(state), "%s/state/starts", data) >= (int)sizeof(state)) return 1;
        FILE *file = fopen(state, "r");
        if (file) { if (fscanf(file, "%lu", &count) != 1) count = 0; fclose(file); }
        file = fopen(state, "w"); if (!file) return 1;
        fprintf(file, "%lu", count + 1); if (fclose(file)) return 1;
        struct sockaddr_un address = {0};
        address.sun_family = AF_UNIX;
        if (!directory || snprintf(address.sun_path, sizeof(address.sun_path), "%s/service.sock", directory) >= (int)sizeof(address.sun_path)) return 1;
        int server = socket(AF_UNIX, SOCK_STREAM, 0);
        unlink(address.sun_path);
        if (server < 0 || bind(server, (struct sockaddr *)&address, sizeof(address)) || listen(server, 8)) return 1;
        puts("hello C service ready"); fflush(stdout);
        for (;;) {
            int client = accept(server, NULL, NULL);
            if (client < 0) return 1;
            struct timeval timeout = {1, 0};
            setsockopt(client, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
            char request[14];
            if (read(client, request, sizeof(request)) > 0) { (void)write(client, "OK\n\n", 4); }
            close(client);
        }
    }
    puts("Hello from native ZionOS C, built with Forge!");
    return 0;
}
