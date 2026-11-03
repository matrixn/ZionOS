cmake_minimum_required(VERSION 3.20)
project(zion_hello_c VERSION 1.0.0 LANGUAGES C)
add_executable(zion-hello-c hello.c)
install(TARGETS zion-hello-c RUNTIME DESTINATION bin)
