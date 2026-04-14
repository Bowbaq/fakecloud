plugins {
    `java-library`
}

group = "dev.fakecloud"
version = "0.1.0"

repositories {
    mavenCentral()
}

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(17))
    }
    withSourcesJar()
    withJavadocJar()
}

dependencies {
    api("com.fasterxml.jackson.core:jackson-databind:2.17.2")
    api("com.fasterxml.jackson.core:jackson-annotations:2.17.2")

    testImplementation(platform("org.junit:junit-bom:5.10.3"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")

    val awsSdk = "2.27.21"
    testImplementation(platform("software.amazon.awssdk:bom:$awsSdk"))
    testImplementation("software.amazon.awssdk:sqs")
    testImplementation("software.amazon.awssdk:sns")
    testImplementation("software.amazon.awssdk:sesv2")
    testImplementation("software.amazon.awssdk:s3")
    testImplementation("software.amazon.awssdk:dynamodb")
    testImplementation("software.amazon.awssdk:cognitoidentityprovider")
    testImplementation("software.amazon.awssdk:eventbridge")
    testImplementation("software.amazon.awssdk:rds")
    testImplementation("software.amazon.awssdk:elasticache")
}

tasks.test {
    useJUnitPlatform()
    testLogging {
        events("passed", "skipped", "failed")
        showStandardStreams = false
    }
}

tasks.javadoc {
    (options as StandardJavadocDocletOptions).addStringOption("Xdoclint:none", "-quiet")
}
