//go:build !windows

package llmproc

import "syscall"

// processGroupAttr ensures the child runs in its own process group so we can
// signal the whole tree with syscall.Kill(-pid, sig).
func processGroupAttr() *syscall.SysProcAttr {
	return &syscall.SysProcAttr{Setpgid: true}
}

// terminateProcess asks the whole process group to shut down politely.
func terminateProcess(pid int) error {
	return syscall.Kill(-pid, syscall.SIGTERM)
}

// killProcess force-kills the whole process group.
func killProcess(pid int) error {
	return syscall.Kill(-pid, syscall.SIGKILL)
}
