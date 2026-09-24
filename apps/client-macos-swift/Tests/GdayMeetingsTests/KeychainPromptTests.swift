import Foundation
import Security
import Testing
@testable import GdayMeetings

struct KeychainPromptTests {
    @Test func renamingPreservesAccessPolicyAndNonPromptMetadata() throws {
        // In-memory ACL only: never opens a keychain or reads real credentials.
        var created: SecAccess?
        #expect(SecAccessCreate("Old prompt" as CFString, [] as CFArray, &created) == errSecSuccess)
        let access = try #require(created)
        var list: CFArray?
        #expect(SecAccessCopyACLList(access, &list) == errSecSuccess)
        let entries = try #require(list as? [SecACL])
        struct Snapshot {
            let apps: CFArray?
            let description: String?
            let flags: SecKeychainPromptSelector
            let authorizations: CFArray
        }
        func snapshot(_ entry: SecACL) throws -> Snapshot {
            var apps: CFArray?
            var description: CFString?
            var flags = SecKeychainPromptSelector(rawValue: 0)
            #expect(SecACLCopyContents(entry, &apps, &description, &flags) == errSecSuccess)
            return Snapshot(apps: apps, description: description as String?, flags: flags,
                            authorizations: SecACLCopyAuthorizations(entry))
        }
        let before = try entries.map(snapshot)
        let label = KeychainPrompt.label("transcription-api-key")
        #expect(try KeychainPrompt.rename(access, to: label))
        for (entry, old) in zip(entries, before) {
            let new = try snapshot(entry)
            #expect(new.flags == old.flags)
            #expect(CFEqual(new.authorizations, old.authorizations))
            if let apps = old.apps {
                #expect(new.apps != nil && CFEqual(apps, new.apps!))
            } else { #expect(new.apps == nil) }
            let isPrompt = (old.authorizations as! [String]).contains(kSecACLAuthorizationDecrypt as String)
            #expect(new.description == (isPrompt ? label : old.description))
        }
        #expect(try !KeychainPrompt.rename(access, to: label))
    }
}
