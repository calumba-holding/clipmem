import Foundation

struct CommandResult: Sendable {
    var exitCode: Int32
    var stdout: Data
    var stderr: Data

    var stdoutText: String {
        String(data: stdout, encoding: .utf8) ?? ""
    }

    var stderrText: String {
        String(data: stderr, encoding: .utf8) ?? ""
    }
}

struct CommandRunner: Sendable {
    func run(executable: String, arguments: [String]) async throws -> CommandResult {
        let runningProcess = RunningProcess()
        let cancellationState = CancellationState()
        return try await withTaskCancellationHandler {
            try await Task.detached(priority: .userInitiated) {
                let process = Process()
                let stdout = Pipe()
                let stderr = Pipe()
                let stdoutReader = PipeReader(fileHandle: stdout.fileHandleForReading)
                let stderrReader = PipeReader(fileHandle: stderr.fileHandleForReading)

                process.executableURL = URL(fileURLWithPath: executable)
                process.arguments = arguments
                process.standardOutput = stdout
                process.standardError = stderr
                runningProcess.set(process)
                defer { runningProcess.clear() }

                stdoutReader.start()
                stderrReader.start()
                do {
                    try cancellationState.checkCancellation()
                } catch {
                    stdout.fileHandleForWriting.closeFile()
                    stderr.fileHandleForWriting.closeFile()
                    _ = stdoutReader.wait()
                    _ = stderrReader.wait()
                    throw error
                }

                do {
                    try process.run()
                } catch {
                    stdout.fileHandleForWriting.closeFile()
                    stderr.fileHandleForWriting.closeFile()
                    _ = stdoutReader.wait()
                    _ = stderrReader.wait()
                    throw error
                }

                process.waitUntilExit()
                let stdoutData = stdoutReader.wait()
                let stderrData = stderrReader.wait()
                try cancellationState.checkCancellation()
                return CommandResult(exitCode: process.terminationStatus, stdout: stdoutData, stderr: stderrData)
            }.value
        } onCancel: {
            cancellationState.cancel()
            runningProcess.terminate()
        }
    }
}

// Accessed by a cancellation handler and a worker task, so access is synchronized.
private final class RunningProcess: @unchecked Sendable {
    private let lock = NSLock()
    private var process: Process?

    func set(_ process: Process) {
        lock.lock()
        self.process = process
        lock.unlock()
    }

    func terminate() {
        lock.lock()
        let process = process
        lock.unlock()
        process?.terminate()
    }

    func clear() {
        lock.lock()
        process = nil
        lock.unlock()
    }
}

private final class CancellationState: @unchecked Sendable {
    private let lock = NSLock()
    private var cancelled = false

    func cancel() {
        lock.lock()
        cancelled = true
        lock.unlock()
    }

    func checkCancellation() throws {
        lock.lock()
        let cancelled = cancelled
        lock.unlock()
        if cancelled {
            throw CancellationError()
        }
    }
}

private final class PipeReader: @unchecked Sendable {
    private let fileHandle: FileHandle
    private let semaphore = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var data = Data()

    init(fileHandle: FileHandle) {
        self.fileHandle = fileHandle
    }

    func start() {
        DispatchQueue.global(qos: .userInitiated).async {
            let output = self.fileHandle.readDataToEndOfFile()
            self.lock.lock()
            self.data = output
            self.lock.unlock()
            self.semaphore.signal()
        }
    }

    func wait() -> Data {
        semaphore.wait()
        lock.lock()
        let output = data
        lock.unlock()
        return output
    }
}
