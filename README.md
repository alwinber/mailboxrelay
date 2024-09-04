# Mailrelay
A simple mail retrieval agent that retrieves and forwards your mail to a different address.  
This is usefull when you want to migrate away from a mail provider that charges for automatic forwarding.

## Usage
```
$ mailrelay -h                  
A mail retrieval agent that retrieves email using IMAP and forwards it to a different address using SMTP

Usage: mailrelay [OPTIONS] --config <CONFIG>

Options:
  -c, --config <CONFIG>      Path to the config file with login information
  -i, --interval <INTERVAL>  The interval in seconds to check for new emails. Use 0 for oneshot [default: 600]
  -h, --help                 Print help
  -V, --version              Print version
```

## Configuration
Config files look lile this:
```toml
[someaccount]
imap_domain = "imap.example.com"
imap_username = "user@example.com"
imap_password = "p4ssw0rd"
smtp_domain = "smtp.example.com"
smtp_username = "user@example.com"
smtp_password = "p4ssw0rd"
mailboxes = ["INBOX", "Junk"]
forward_target = "you@yourdomain.tld"
```