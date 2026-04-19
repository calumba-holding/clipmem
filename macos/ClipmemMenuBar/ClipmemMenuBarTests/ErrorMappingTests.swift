import Foundation
import Testing
@testable import ClipmemMenuBar

struct ErrorMappingTests {
    @Test func errorDescriptionsAreUserFacing() {
        let error = ClipmemClientError.setupNeeded("database does not exist")

        #expect(error.localizedDescription == "database does not exist")
    }

    @Test func binaryNotFoundDescriptionIsStable() {
        let error = ClipmemClientError.binaryNotFound(["/missing/clipmem"])

        #expect(error.localizedDescription == "clipmem binary was not found.")
    }
}
