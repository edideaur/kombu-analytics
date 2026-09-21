# Init System Configurations for Kombu Analytics

Production service configurations and auto recovery handlers for major init systems.

## Systemd (Linux)

Install the service file and environment file:

```bash
sudo cp init/systemd/kombu.service /etc/systemd/system/
sudo mkdir -p /etc/kombu
sudo cp init/systemd/kombu.env /etc/kombu/kombu.env
sudo systemctl daemon-reload
sudo systemctl enable --now kombu
```

Auto restart is configured with `Restart=always` and `RestartSec=3s`.

## OpenRC (Alpine Linux, Gentoo)

Install OpenRC service and configuration:

```bash
sudo cp init/openrc/kombu.initd /etc/init.d/kombu
sudo cp init/openrc/kombu.confd /etc/conf.d/kombu
sudo chmod +x /etc/init.d/kombu
sudo rc-update add kombu default
sudo rc-service kombu start
```

Supervise daemon ensures automatic restart on unexpected exit (`respawn_max=0`).

## Runit (Void Linux, Artix)

Install the runit service directory:

```bash
sudo mkdir -p /etc/sv/kombu
sudo cp init/runit/run init/runit/finish /etc/sv/kombu/
sudo chmod +x /etc/sv/kombu/run /etc/sv/kombu/finish
sudo ln -s /etc/sv/kombu /var/service/
```

## s6 (s6-overlay, Alpine s6)

Install the s6 service directory:

```bash
sudo mkdir -p /etc/services.d/kombu
sudo cp init/s6/run init/s6/finish init/s6/type /etc/services.d/kombu/
sudo chmod +x /etc/services.d/kombu/run /etc/services.d/kombu/finish
```

## SysVinit (Debian legacy, Devuan)

Install the SysVinit script:

```bash
sudo cp init/sysvinit/kombu /etc/init.d/kombu
sudo chmod +x /etc/init.d/kombu
sudo update-rc.d kombu defaults
sudo /etc/init.d/kombu start
```

Health check verification:

```bash
/etc/init.d/kombu health
```

## Launchd (macOS)

Install the launch daemon for native macOS operation:

```bash
sudo cp init/launchd/com.kombu.analytics.plist /Library/LaunchDaemons/
sudo launchctl load -w /Library/LaunchDaemons/com.kombu.analytics.plist
```

## FreeBSD rc.d

Install the rc.d script:

```bash
sudo cp init/freebsd/kombu /usr/local/etc/rc.d/kombu
sudo chmod +x /usr/local/etc/rc.d/kombu
sudo sysrc kombu_enable="YES"
sudo service kombu start
```

## Process Watchdogs (Monit and Supervisord)

* Monit configuration: `init/monit/kombu.monitrc` actively checks `http://127.0.0.1:3000/api/health` and restarts the process upon consecutive failures.
* Supervisord configuration: `init/supervisord/kombu.conf` provides `autorestart=true` with process management.
