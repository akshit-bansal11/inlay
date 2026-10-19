# winget

The manifest that puts Inlay in the Windows Package Manager as `AkshitBansal.Inlay`. It is a `portable` package: winget downloads the release's exe as it is, puts an `inlay` command on the PATH, and replaces the file on `winget upgrade`.

The files here are a record of what is to be submitted. winget does not read them from this repository; a package exists once a pull request adding them is merged into [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs). Submitting is the maintainer's to do, because the pull request is opened from his account.

## The rename

Versions 1.0.0 to 1.4.0 were prepared as `AkshitBansal.Deskbeat` but never accepted into winget-pkgs; their folders are kept for history. 2.0.0 is the manifest to submit. 2.1.0's manifest follows once its release exists, because its `InstallerSha256` is only known then. From the first release named Inlay, the folder holds `AkshitBansal.Inlay.*` files with `PackageName: Inlay`, and `submit.sh` submits it as a new package. Once it is in, the old package can be retired by a pull request to winget-pkgs that removes `AkshitBansal.Deskbeat`.

## Submitting

```bash
winget validate --manifest packaging/winget/2.0.0
bash packaging/winget/submit.sh 2.0.0
```

`submit.sh` forks `winget-pkgs` if there is no fork yet, writes the three files to a branch of it through the GitHub API, and opens the pull request with `gh`. Nothing is cloned. Microsoft's `wingetcreate` does the same job and is not used here, because it needs the .NET runtime.

On the pull request, a bot asks for Microsoft's Contributor License Agreement the first time; it is signed by replying there. Microsoft's checks then download and scan the exe, and a person approves a new package.

## Each later release

Copy the newest folder to one named for the new version, and in its three files change `PackageVersion`, the version in `InstallerUrl` and `ReleaseNotesUrl`, `ReleaseDate`, and `InstallerSha256`, which is the exe's line in the release's `SHA256SUMS.txt`, in capitals. Then validate and submit as above.

## Not yet known

None of this has been through Microsoft's checks, and `winget validate` has not been run on these files, so these are open until the first submission:

- **Whether an unsigned exe passes.** The exe is not code-signed, and the validation run includes a malware scan and a SmartScreen check.
- **Whether Start with Windows survives an upgrade.** The app writes its own path into the Run key. If winget keeps the versioned file name, the path changes with each release and the switch needs turning on again.
- **Whether `winget upgrade` can replace the exe while the tray app is running.** Quit Inlay first if it cannot.
