//
//  AuthToken.swift
//  
//
//  Created by Nora Trapp on 3/29/23.
//

import Foundation
import LoamSdkFfi

public struct AuthToken {
    public let tenant: String
    public let user: String
    public let signature: Data

    func withUnsafeFfi<Result>(_ body: (LoamAuthToken) throws -> Result) rethrows -> Result {
        try tenant.withCString { tenantCStr in
            try user.withCString { userCStr in
                try signature.withLoamUnmanagedDataBuffer { signatureBuffer in
                    try body(.init(
                        tenant: tenantCStr,
                        user: userCStr,
                        signature: signatureBuffer
                    ))
                }
            }
        }
    }
}
