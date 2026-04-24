plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.maven.publish)
    alias(libs.plugins.tapmoc)
}

android {
    namespace = "tachiyomi.decoder"

    compileSdk = 37

    defaultConfig {
        minSdk = 21

        consumerProguardFiles("consumer-rules.pro")
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }
}

tapmoc {
    java(17)
}

mavenPublishing {
    coordinates("com.github.mihonapp", "image-decoder", "1.0.0-SNAPSHOT")
}
