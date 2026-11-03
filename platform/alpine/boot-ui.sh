# Shared console renderer. Sourcing and the initramfs intro use shell builtins only.
# Animation runs only around OpenRC; no process survives a runlevel transition.
zion_cyan="$(printf '\033[1;36m')"
zion_blue="$(printf '\033[1;34m')"
zion_pink="$(printf '\033[1;35m')"
zion_white="$(printf '\033[1;37m')"
zion_dim="$(printf '\033[2m')"
zion_reset="$(printf '\033[0m')"

zion_ui_line() { printf '\033[2K  %s\n' "$1"; }

zion_boot_frame() {
    zion_phase=$1 zion_step=$2 zion_frame=$3 zion_recent=${4:-}
    case "$zion_phase" in
        initramfs) zion_title='EARLY USERSPACE'; zion_detail='Discovering boot media and preparing the live root.';;
        sysinit) zion_title='SYSTEM FOUNDATION'; zion_detail='Device nodes, hardware drivers and the module image.';;
        boot) zion_title='NETWORK & IDENTITY'; zion_detail='Hostname, system services and the DHCP connection.';;
        default) zion_title='ZION CORE'; zion_detail='Starting the read-only API and the browser desktop.';;
        ready) zion_title='YOUR NAS IS ONLINE'; zion_detail='Zion Core health verified. Welcome to your space.';;
        pending) zion_title='OPENRC COMPLETE'; zion_detail='Services started. HTTP API readiness is still pending.';;
    esac
    case $((zion_frame % 4)) in
        0) zion_glow=$zion_cyan zion_spinner='/' ;;
        1) zion_glow=$zion_blue zion_spinner='-' ;;
        2) zion_glow=$zion_pink zion_spinner='\' ;;
        3) zion_glow=$zion_white zion_spinner='|' ;;
    esac
    printf '\033[H'
    zion_ui_line "${zion_dim}GENESIS 0.7.0  /  ALPINE 3.23  /  LEGACY x86-64${zion_reset}"
    zion_ui_line "${zion_glow}--------------------------------------------------------------------${zion_reset}"
    zion_ui_line ''
    zion_ui_line "${zion_glow} ZZZZZZZZ  IIII   OOOOO   NN    NN    OOOOO    SSSSSS${zion_reset}"
    zion_ui_line "${zion_glow}      ZZ    II   OO   OO  NNN   NN   OO   OO  SS${zion_reset}"
    zion_ui_line "${zion_glow}    ZZ      II   OO   OO  NN NN NN   OO   OO   SSSSS${zion_reset}"
    zion_ui_line "${zion_glow}  ZZ        II   OO   OO  NN  NNNN   OO   OO       SS${zion_reset}"
    zion_ui_line "${zion_glow} ZZZZZZZZ  IIII   OOOOO   NN    NN    OOOOO    SSSSSS${zion_reset}"
    zion_ui_line ''
    zion_ui_line "${zion_white}             Y O U R   S P A C E .   I N   C O N T R O L .${zion_reset}"
    zion_ui_line "${zion_glow}--------------------------------------------------------------------${zion_reset}"
    zion_ui_line "${zion_white}0${zion_step} / 04   ${zion_title}${zion_reset}"
    zion_ui_line "$zion_detail"
    printf '\033[2K  %s[' "$zion_glow"
    zion_cell=0
    while [ "$zion_cell" -lt 48 ]; do
        zion_distance=$(( (zion_cell - zion_frame % 48 + 48) % 48 ))
        if [ "$zion_phase" = ready ]; then printf '='
        elif [ "$zion_distance" -lt 7 ]; then printf '='
        else printf '.'; fi
        zion_cell=$((zion_cell + 1))
    done
    printf '] %s%s\n' "$zion_spinner" "$zion_reset"
    if [ "$zion_phase" = ready ]; then
        zion_ui_line "${zion_cyan}DESKTOP  http://localhost:8080  (QEMU host)${zion_reset}"
    else
        zion_ui_line "${zion_dim}Activity indicator / milestone progress / no estimated percentage${zion_reset}"
    fi
    zion_ui_line "${zion_dim}01 LIVE ROOT  >  02 HARDWARE  >  03 NETWORK  >  04 CORE${zion_reset}"
    zion_ui_line "${zion_glow}--------------------------------------------------------------------${zion_reset}"
    zion_ui_line "${zion_dim}LIVE BOOT TELEMETRY${zion_reset}"
    # Clip diagnostics to the panel width; control bytes are removed by the caller.
    zion_rows=0
    while IFS= read -r zion_log_line; do
        [ "$zion_rows" -lt 4 ] || break
        printf '\033[2K  %s%.68s%s\n' "$zion_dim" "$zion_log_line" "$zion_reset"
        zion_rows=$((zion_rows + 1))
    done <<ZION_TELEMETRY
$zion_recent
ZION_TELEMETRY
    while [ "$zion_rows" -lt 4 ]; do zion_ui_line ''; zion_rows=$((zion_rows + 1)); done
    printf '\033[2K  %sStorage HTTP: read-only  /  Diagnostics: zion.splash=off%s' "$zion_dim" "$zion_reset"
}

zion_boot_animate() {
    trap 'exit 0' HUP INT TERM
    zion_tick=0
    while kill -0 "$zion_owner" 2>/dev/null; do
        # Keep the hero fixed. A full-screen repaint at 4 Hz floods a 115200 UART.
        case $((zion_tick % 4)) in
            0) zion_glow=$zion_cyan zion_spinner='/';;
            1) zion_glow=$zion_blue zion_spinner='-';;
            2) zion_glow=$zion_pink zion_spinner='\';;
            3) zion_glow=$zion_white zion_spinner='|';;
        esac
        zion_bar='' zion_cell=0
        while [ "$zion_cell" -lt 48 ]; do
            zion_distance=$(( (zion_cell - zion_tick % 48 + 48) % 48 ))
            if [ "$zion_distance" -lt 7 ]; then zion_bar="${zion_bar}="
            else zion_bar="${zion_bar}."; fi
            zion_cell=$((zion_cell + 1))
        done
        printf '\033[14;1H\033[2K  %s[%s] %s%s' "$zion_glow" "$zion_bar" "$zion_spinner" "$zion_reset"
        if [ $((zion_tick % 4)) -eq 0 ]; then
            zion_recent="$(tail -n 4 "$zion_log" 2>/dev/null | tr -cd '\12\40-\176')"
            printf '\033[19;1H'
            zion_rows=0
            while IFS= read -r zion_log_line; do
                [ "$zion_rows" -lt 4 ] || break
                printf '\033[2K  %s%.68s%s\n' "$zion_dim" "$zion_log_line" "$zion_reset"
                zion_rows=$((zion_rows + 1))
            done <<ZION_TELEMETRY
$zion_recent
ZION_TELEMETRY
            while [ "$zion_rows" -lt 4 ]; do zion_ui_line ''; zion_rows=$((zion_rows + 1)); done
        fi
        printf '\033[24;1H'
        zion_tick=$((zion_tick + 1))
        sleep 0.25 || break
    done
}
