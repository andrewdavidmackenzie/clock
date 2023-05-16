## Clock

A blatant copy of the iced clock example into a standalone app, to use to port a flutter clock app I was
working on and to learned iced GUI in the process.

It uses the `Canvas` widget to draw a clock and its hands to display the current time.

If it turns out nice, I will offer it back to iced as an evolved example.

<div align="center">
  <img src="https://user-images.githubusercontent.com/518289/74716344-a3e6b300-522e-11ea-8aea-3cc0a5100a2e.gif">
</div>

You can run it with:
```
cargo run
```

### Build notes
On Raspberry Pi OS I had a problem with `fontconfig` and `pkgcfg`, and had to install these packages related to fonts before I could build:

```
sudo apt-get install libfreetype6-dev libfontconfig1-dev xclip
```