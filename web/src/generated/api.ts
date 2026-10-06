/**
 * 本文件由 openapi-typescript 依据 docs/api/openapi.yaml 生成，请勿手工修改。
 * 重新生成：npm run api:generate
 * 校验一致性：npm run api:check
 */
export interface paths {
    "/api/v1/audit": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 查询审计记录
         * @description 审计只记元数据，detail 依约定不含凭据、环境变量、SQL 正文与文件内容。
         */
        get: {
            parameters: {
                query?: {
                    /** @description 按类别精确筛选 */
                    category?: string;
                    /** @description 时间下界（Unix 秒），at >= from */
                    from?: number;
                    /** @description 返回条数上限，默认 200，硬上限 1000 */
                    limit?: number;
                    /** @description 按 node_id 精确筛选 */
                    node?: string;
                    /** @description 按结果精确筛选 */
                    result?: string;
                    /** @description 时间上界（Unix 秒），at <= to */
                    to?: number;
                };
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 审计记录与保留期。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            records: components["schemas"]["AuditRecord"][];
                            /**
                             * @description 审计保留时长，恒为 365 天秒数
                             * @constant
                             */
                            retention_seconds: 31536000;
                        };
                    };
                };
                400: components["responses"]["BadRequest"];
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/auth/login": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 管理员登录
         * @description 入口之下但不需要会话。自身先做一次 Origin 校验，并按来源 IP 限流。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["LoginRequest"];
                };
            };
            responses: {
                /** @description 登录成功，同时下发 opsd_session Cookie。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Session"];
                    };
                };
                /**
                 * @description 业务校验失败为 {"error":"<中文消息>"}（见 BadRequest）；
                 *     请求体不是 JSON / 含未知字段 / 类型不符时为框架纯文本（见 BadRequest 的 text/plain 侧与 415/422）。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                /** @description 密码错误。 */
                401: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description Origin 不匹配，返回 {"error":"请求来源不匹配"}。这是登录自身的校验，早于其他检查。 */
                403: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
                /** @description 登录过于频繁或来源过多。正文为「登录尝试过于频繁」或「请稍后重试」。 */
                429: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/auth/logout": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 退出登录
         * @description 删除服务端会话并清除 Cookie。无请求体。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 已退出。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Ok"];
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/auth/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 读取当前会话
         * @description GET 不做 CSRF/Origin 校验，只校验会话存在。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 当前会话。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Session"];
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/database/overview": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 数据库只读巡检总览
         * @description 只读视图:按名称排序返回各节点结论(不含已撤销节点)、跨节点一致性检查结论与汇总。
         *     整个视图不含任何凭据类字段:configured 只是「已配置只读账号」的布尔标记,报告与审计均无
         *     账号、密码、令牌字段,Agent 侧凭据只存在于节点本机。
         *     未知项如实单列,绝不冒充正常:「未知不等于 0」由 null 值与 unknown 列表表达,不补 0。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 巡检总览。expected_cluster_size 取各报告的第一个非零期望成员数,全部为 0 时此值为 0。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Overview"];
                    };
                };
                400: components["responses"]["BadRequest"];
                401: components["responses"]["Unauthorized"];
                /** @description 巡检缓存不可用,正文 {"error":"巡检缓存不可用"}。内存巡检缓存读锁中毒时出现,正常情况不可达。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/enroll": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Agent 一次性注册
         * @description 入口之外的白名单路径，凭一次性令牌放行，不依赖会话。Agent 在拿到客户端证书之前调用。
         *     成功后令牌立即失效（防重放），并签发 90 天客户端证书。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["EnrollRequest"];
                };
            };
            responses: {
                /** @description 节点编号与客户端证书。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["EnrollResponse"];
                    };
                };
                /** @description 令牌已被使用、CSR 签名失败或 CA 文件缺失。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                /** @description 注册令牌无效或过期。 */
                401: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/enrollment-tokens": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** 读取待接入节点 */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 待接入摘要，不包含令牌明文。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["EnrollmentSummary"][];
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        /**
         * 签发一次性注册令牌
         * @description 令牌 600 秒内有效、一次性使用，绑定到新生成的 node_id。每次调用都新建令牌，不参与幂等。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["Registration"];
                };
            };
            responses: {
                /** @description 令牌与 CA 指纹。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["EnrollmentToken"];
                    };
                };
                /** @description 业务校验失败 {"error"}；JSON 语法错误为框架纯文本。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/enrollment-tokens/{node_id}": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                node_id: string;
            };
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** 撤销待接入令牌 */
        delete: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    node_id: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 已撤销。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Ok"];
                    };
                };
                400: components["responses"]["BadRequest"];
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/events": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 控制台事件流
         * @description SSE 流（text/event-stream）。每帧 data 是一个带 type 判别字段的 JSON 对象：
         *     tasks / nodes / metrics / storage / database / task（带完整任务对象）/ resync（订阅滞后时提示重新拉取）。
         *     没有重连补传机制：断线期间的事件不会补发，客户端必须重新拉取状态。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description SSE 流。流在会话过期时长后由服务端关闭。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "text/event-stream": components["schemas"]["ConsoleEvent"];
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/metrics/overview": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 各节点最新指标总览
         * @description 只读内存中的最近指标，不写库、不生成任务、不广播事件。只列出真正上报过指标的节点，未上报的节点不出现（缺失而非 0）。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 最新指标与时钟偏移告警阈值。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            /**
                             * @description 时钟偏移告警阈值（秒），固定 60；偏移超过它提示管理员。
                             * @constant
                             */
                            clock_skew_warn_seconds: 60;
                            /** @description 所有已上报节点的最近成功采样与当前采集状态。 */
                            nodes: components["schemas"]["MetricsOverviewNode"][];
                        };
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/metrics/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 指标层级保留状态
         * @description 只读汇总各层级（raw / m5 / h1）的记录数与最早时间，用于观察保留策略是否按预期生效。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 三个层级的汇总，每项一条。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            tiers: components["schemas"]["MetricsTierInfo"][];
                        };
                    };
                };
                400: components["responses"]["BadRequest"];
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/nodes": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 节点列表
         * @description 返回全部节点（含已撤销）及其在线判定。在线判定由 last_seen 与在线通道共同决定，不在 Node 结构体内。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 节点条目数组。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["NodeEntry"][];
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/nodes/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                id: components["parameters"]["NodeId"];
            };
            cookie?: never;
        };
        get?: never;
        /**
         * 更新节点
         * @description 更新节点登记信息，随后向未撤销节点重新下发地址与探测目标任务。
         */
        put: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                    id: components["parameters"]["NodeId"];
                };
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["Registration"];
                };
            };
            responses: {
                /** @description 更新后的节点。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Node"];
                    };
                };
                /** @description 业务校验失败 {"error"}；JSON 语法错误为框架纯文本。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                413: components["responses"]["PayloadTooLarge"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        post?: never;
        /**
         * 撤销节点
         * @description 置 revoked=true 并断开在线通道，不物理删除记录。
         */
        delete: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                    id: components["parameters"]["NodeId"];
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 已撤销。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Ok"];
                    };
                };
                400: components["responses"]["BadRequest"];
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/nodes/{id}/actions": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                id: components["parameters"]["NodeId"];
            };
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 提交节点操作
         * @description 所有节点变更的统一入口。提交前校验幂等键长度、节点存在/未撤销/在线，
         *     并拒绝外部提交 peer_sync（地址集合只能由节点目录生成）。
         *     返回 202 与 task_id；任务由调度循环投递给节点。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                    id: components["parameters"]["NodeId"];
                };
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["Submit"];
                };
            };
            responses: {
                /** @description 已受理。重放相同幂等键与相同参数时返回同一 task_id。 */
                202: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["TaskAccepted"];
                    };
                };
                /**
                 * @description 参数或状态不满足：幂等键长度无效、节点不可用、节点离线、地址集合只能由节点目录生成、
                 *     验证节点必须是另一个在线节点、幂等键已用于不同参数。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/nodes/{id}/db-inspect": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                id: components["parameters"]["NodeId"];
            };
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 下发数据库只读巡检
         * @description 创建一次只读巡检任务:新建且节点在线时立即经 Agent 长连接下发并广播 database 事件,
         *     离线时留待调度;不校验节点是否存在。幂等键去重:同节点 + 同键复用既有任务并返回
         *     同一 task_id,不重复下发。审计记录只含对象与结果,不含 SQL 正文与凭据。
         *     注意返回 200 而不是约定中的 202(历史原因,与文件面一致),task_id 可用于查询
         *     /api/v1/tasks/{id} 的执行结果。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                    id: components["parameters"]["NodeId"];
                };
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["DbInspectRequest"];
                };
            };
            responses: {
                /** @description 已受理,正文为 {"task_id":"<任务 UUID>"}。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["TaskAccepted"];
                    };
                };
                /**
                 * @description 拒绝受理,正文为 {"error":"<中文消息>"}:同节点同键但参数摘要不同时报
                 *     「幂等键已用于不同参数」(历史原因 400 而非 409);accept_task 其余失败
                 *     (参数摘要不匹配、写入错误)同样映射为 400。请求体不是合法 JSON、字段类型不匹配
                 *     或含未知字段时,见 components/responses/BadRequest(纯文本)。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                413: components["responses"]["PayloadTooLarge"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/nodes/{id}/files": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                id: components["parameters"]["NodeId"];
            };
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 提交结构化文件操作
         * @description 只接受 put / remove / rename / mkdir / chmod / chown 六类结构化操作，不接受任意命令。
         *     注意与 actions 的差异：本端点**不校验节点是否存在或在线**，节点不存在也会落库为待投递任务；
         *     成功返回 200（不是 202），并写审计。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                    id: components["parameters"]["NodeId"];
                };
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["FileSubmit"];
                };
            };
            responses: {
                /** @description 已受理。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["TaskAccepted"];
                    };
                };
                /** @description 业务校验失败 {"error"}；JSON 语法错误为框架纯文本。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/nodes/{id}/host-inspect": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                id: components["parameters"]["NodeId"];
            };
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 触发节点主机盘点
         * @description 只读操作但走任务机制以便看到执行结果：写库登记 HostInspect 任务，任务为 Pending 时向该节点通道下发，
         *     并广播 events。成功返回 200 + task_id（不是 202）。幂等键用于去重同一个盘点动作。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                    id: components["parameters"]["NodeId"];
                };
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["HostInspectRequest"];
                };
            };
            responses: {
                /** @description 已接受任务的 id，供任务面板回溯。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["TaskAccepted"];
                    };
                };
                /**
                 * @description 同节点同 idempotency_key 已用于参数不同的动作时返回「幂等键已用于不同参数」；
                 *     其他错误（如任务入库失败）返回对应 DB 错误信息。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/nodes/{id}/metrics": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 节点指标历史
         * @description 只读指标表：按步长选择层级（step<300 用 raw，<3600 用 m5，其余用 h1）并抽稀。
         *     降采样桶内标量取平均、明细取最后一次；区间内没有数据的时段不插值、不补零，直接缺空。
         */
        get: {
            parameters: {
                query: {
                    /** @description 起始时刻（Unix 秒）。 */
                    from: number;
                    /** @description 期望点间距（秒），默认 60；实际值被 clamp 到 [15, 86400]，同时决定降采样层级。 */
                    step?: number;
                    /** @description 结束时刻（Unix 秒），必须晚于 from。 */
                    to: number;
                };
                header?: never;
                path: {
                    /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                    id: components["parameters"]["NodeId"];
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 抽稀后的指标点列与查询元信息。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            /**
                             * Format: int64
                             * @description 请求的起始时刻（Unix 秒）。
                             */
                            from: number;
                            /** @description 路径中的节点编号。 */
                            node_id: string;
                            points: components["schemas"]["MetricsPoint"][];
                            /** @description clamp 到 [15, 86400] 后的实际步长（秒）。 */
                            step: number;
                            /**
                             * @description 实际使用的层级，由 clamp 后的步长决定。
                             * @enum {string}
                             */
                            tier: "raw" | "m5" | "h1";
                            /**
                             * Format: int64
                             * @description 请求的结束时刻（Unix 秒）。
                             */
                            to: number;
                            /** @description 是否因超过单次查询点数上限 MAX_POINTS（2000）而截断。 */
                            truncated: boolean;
                        };
                    };
                };
                /**
                 * @description 参数不满足或查询失败：to<=from 时正文为 {"error":"结束时间必须晚于开始时间"}；
                 *     (to-from)/step 超过 MAX_POINTS（2000）时正文为 {"error":"区间与步长组合返回点数过多"}；
                 *     数据库查询失败时由内部错误统一映射为 400。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/nodes/{id}/stream": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 节点会话流（WebSocket 升级，非 SSE）
         * @description 建立到节点的实时会话通道。这是 WebSocket 升级（101）而不是 SSE：GET 由门禁免 CSRF 放行到处理器，
         *     处理器内再校验 Origin 头精确等于主控来源且 csrf 查询参数等于会话 CSRF，不满足在升级前返回 400
         *     「终端来源或授权无效」。连接建立后上下行均为文本帧，帧不携带类型标识，客户端按会话阶段自行判定。
         *
         *     **帧 payload 一律是 base64（STANDARD），上下行都是**：主控原样转发字符串，节点侧统一解码。
         *     解码后按 kind 分别是——container / shell 为终端字节流（文本按 UTF-8 解读）；
         *     file_list 为一帧 JSON {"path":..., "entries":[...]}；file_read 为一帧原始字节；
         *     file_write 的上行是待写入的原始字节分块，写完后再收到一帧解码后为 JSON 收据
         *     {"staged":..., "size":..., "digest":...}。文件内容在解码后才是原始字节，不要按文本直接使用帧内容。
         *
         *     帧大小上限 65536 字节只作用于**上行**（服务端对接收帧的限制）；下行单帧没有这个上限，
         *     file_read 默认一次可送出 1 MiB，base64 后约 1.4 MB 字符。
         *
         *     会话结束（节点侧 StreamClose 触发）时，服务端先发一个**明文** JSON 文本帧
         *     {"error": <string|null>} 再关闭连接——这是唯一的非 base64 下行帧；
         *     socket 关闭、Agent 流关闭或会话超时（服务端按会话剩余时长限时）同样结束会话。
         *     HostShell / FileList / FileRead / FileWrite 会话开启与结束时各写一条审计（已开始 / 已结束，含传输量与时长）。
         */
        get: {
            parameters: {
                query: {
                    /**
                     * @description 容器终端内执行的命令。只接受 "/bin/sh" 或 "/bin/bash"，并作为单元素 argv 传递；
                     *     缺省按 ["/bin/sh"] 处理。其它值会在节点侧被拒绝（「首版容器终端仅支持 sh 或 bash」）并结束会话。
                     */
                    command?: string;
                    /** @description 仅在 kind=container 时必填，容器编号。 */
                    container?: string;
                    /** @description 会话 CSRF 值，必须等于 Session.csrf。GET 下 CSRF 走查询参数传递。 */
                    csrf: string;
                    /** @description 流类型。 */
                    kind: "container" | "shell" | "file_list" | "file_read" | "file_write";
                    /** @description 仅在 kind=file_read / file_write 时使用，单次读取上限，默认 1 MiB。 */
                    limit?: number;
                    /** @description 仅在 kind=file_read / file_write 时使用，默认 0。 */
                    offset?: number;
                    /** @description 仅在 kind=file_list / file_read / file_write 时必填，文件或目录路径。 */
                    path?: string;
                    /** @description 仅在 kind=shell 时使用，会话起始目录。 */
                    workdir?: string;
                };
                header?: never;
                path: {
                    /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
                    id: components["parameters"]["NodeId"];
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /**
                 * @description WebSocket 升级成功。握手鉴权：会话 Cookie 通过门禁、Origin 头等于主控来源、
                 *     csrf 查询参数等于会话 CSRF；不满足会在升级前返回 400。
                 *     上下行均为文本帧，payload 一律 base64（STANDARD），不携带帧类型标识，解码规则见操作描述。
                 *     帧大小上限 65536 字节只作用于上行；唯一的非 base64 下行帧是收尾的明文 {"error": <string|null>}。
                 *     节点侧 StreamClose 触发时先发该收尾帧再关闭连接；socket 关闭、Agent 流关闭或会话超时同样结束会话。
                 */
                101: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
                /**
                 * @description 握手或参数不满足，正文为 {"error":"<中文消息>"}：Origin 头不等于主控来源或 csrf 不等于会话 CSRF
                 *     （消息「终端来源或授权无效」）、未知流类型、kind=container 缺容器编号、
                 *     file_list / file_read / file_write 缺 path、或节点当前无活动通道（消息「节点离线」）。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/peer-addresses": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 读取节点地址集合
         * @description 未撤销节点的去重排序公网地址集合，带版本号。
         *     该记录由后台对账任务写入（60 秒周期，失败只记日志）。
         *     **全新控制库在首次对账成功前返回 200 + {version: 0, addresses: []}**，不是 null、也不是 404。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 地址集合；尚未写入过时为 version 0 与空数组。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["PeerAddressSet"];
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/settings/entrance": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** 读取安全入口 */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 当前安全入口；无记录时为空串。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["EntranceValue"];
                    };
                };
                401: components["responses"]["Unauthorized"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        /**
         * 轮换安全入口
         * @description 校验通过后立即生效，旧地址失效，用户需用新入口重新访问。新入口不写日志。
         */
        put: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["EntranceValue"];
                };
            };
            responses: {
                /** @description 生效后的入口与是否发生变化。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["EntranceUpdate"];
                    };
                };
                /** @description 业务校验失败 {"error"}；JSON 语法错误为框架纯文本。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/share/keys": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * API 密钥列表
         * @description 按 created_at 降序返回全部密钥(含已撤销)。**列表只含摘要与元数据,绝不包含明文密钥**——明文只在创建响应中出现一次,之后无法取回。GET 不做 CSRF/Origin 校验。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 密钥记录数组。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ApiKeyList"];
                    };
                };
                400: components["responses"]["BadRequest"];
                401: components["responses"]["Unauthorized"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        put?: never;
        /**
         * 创建 API 密钥
         * @description 生成明文密钥(opsd_ 前缀),控制库只存 sha256 摘要。**明文仅此一次返回,之后无法再取回**。
         *     密钥授权走 DB 直查,不维护内存索引,无需刷新。不产生任务、无需幂等键。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["ApiKeyInput"];
                };
            };
            responses: {
                /** @description 明文密钥与密钥记录。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ApiKeyCreated"];
                    };
                };
                /** @description label trim 后为空或超过 60 字符(「密钥名称长度不合法」),或数据库写入失败(正文为错误信息原文)。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                413: components["responses"]["PayloadTooLarge"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/share/keys/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /**
         * 撤销 API 密钥
         * @description 软删除:revoked 置 true,记录仍留在列表;授权每次读库过滤,撤销立即生效。不产生任务、无需幂等键。
         */
        delete: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description API Key 记录 id */
                    id: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 已撤销,ok 恒为 true。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Ok"];
                    };
                };
                /**
                 * @description id 在密钥桶中查不到(「API Key 不存在」,不存在返回 400 而非 404),
                 *     或数据库读写失败(正文为错误信息原文)。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                413: components["responses"]["PayloadTooLarge"];
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/share/settings": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 读取分享设置
         * @description GET 不做 CSRF/Origin 校验,只校验会话存在;只读进程内共享设置,不写库、不产生任务、无需幂等键。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 当前分享设置:enabled 为分享页整体开关,site 为站点公开信息。读锁失败时回落默认值(enabled=false 与默认站点),不报错。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareSettings"];
                    };
                };
                401: components["responses"]["Unauthorized"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        /**
         * 更新分享设置
         * @description 写 control.db 并同步内存,分享门禁立即生效;不产生任务、无需幂等键,写库失败同其他内部错误一样折算 400。
         */
        put: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["ShareSettingsInput"];
                };
            };
            responses: {
                /** @description 回显写入后的设置。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareSettings"];
                    };
                };
                /**
                 * @description 站点名称超过 60 字符(「站点名称过长」)、站点描述超过 200 字符(「站点描述过长」)、页脚文字超过 200 字符(「页脚文字过长」),
                 *     或数据库写入失败(正文为错误信息原文)。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                413: components["responses"]["PayloadTooLarge"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/share/tokens": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 分享令牌列表
         * @description 按 created_at 降序返回全部令牌(含已撤销)。**列表只含摘要与元数据,绝不包含明文令牌**——明文只在创建响应中出现一次,之后无法取回。GET 不做 CSRF/Origin 校验。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 令牌记录数组。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareTokenList"];
                    };
                };
                400: components["responses"]["BadRequest"];
                401: components["responses"]["Unauthorized"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        put?: never;
        /**
         * 创建分享令牌
         * @description 生成明文令牌,控制库只存 sha256 摘要。**明文仅此一次返回,之后无法再取回**。
         *     落库后重建门禁内存索引,新令牌立即对 /share/{token}/ 生效。不产生任务、无需幂等键。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["ShareTokenInput"];
                };
            };
            responses: {
                /** @description 明文令牌与令牌记录。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareTokenCreated"];
                    };
                };
                /**
                 * @description label trim 后为空或超过 60 字符(「分享名称长度不合法」)、expires_hours 不在 1..=8760 内(「有效时长不合法」),
                 *     或数据库写入/索引重建失败(正文为错误信息原文)。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                413: components["responses"]["PayloadTooLarge"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/share/tokens/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /**
         * 撤销分享令牌
         * @description 软删除:revoked 置 true,记录仍留在列表;重建门禁内存索引,撤销立即生效。不产生任务、无需幂等键。
         */
        delete: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 分享令牌记录 id */
                    id: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 已撤销,ok 恒为 true。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Ok"];
                    };
                };
                /**
                 * @description id 在令牌桶中查不到(「分享令牌不存在」,本项目怪异:不存在返回 400 而非 404),
                 *     或数据库读写/索引重建失败(正文为错误信息原文)。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                413: components["responses"]["PayloadTooLarge"];
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/storage/buckets": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 创建或删除存储桶
         * @description 桶是集群级操作，在任一在线节点执行即可，不逐节点下发。remove=true 时删除桶而非创建。
         *     成功返回 200 + task_id（不是 202），label 固定为「桶」。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["BucketRequest"];
                };
            };
            responses: {
                /** @description 已下发的桶任务。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["StorageTaskAccepted"];
                    };
                };
                /** @description 正文为对应中文消息：集群不存在 / 集群没有在线节点 / 节点通道不可用 / 任务入库失败（DB 错误信息）。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/storage/clusters": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 集群定义列表
         * @description 已定义集群列表；每个集群的 secret_key 一律删除并替换为 has_secret_key 布尔（密钥不回传）。
         *     同时返回固定的镜像仓库前缀、文件系统与最小节点数，界面据此生成选择项。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 集群列表与固定常量。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            clusters: components["schemas"]["ClusterListItem"][];
                            /**
                             * @description 固定文件系统，xfs。
                             * @constant
                             */
                            filesystem: "xfs";
                            /**
                             * @description 允许的镜像仓库前缀，固定为 pgsty/silo。
                             * @constant
                             */
                            image_prefix: "pgsty/silo";
                            /**
                             * @description 分布式部署允许的最小节点数。
                             * @constant
                             */
                            min_nodes: 4;
                        };
                    };
                };
                /** @description 列集群失败，正文为对应 DB 错误信息。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                401: components["responses"]["Unauthorized"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        /**
         * 保存集群定义
         * @description 只写定义，不碰任何节点。created_at 由服务端填写：新建时设为当前时间，已存在则保留旧值。
         *     校验失败返回 400 与中文校验消息。成功返回 200。
         */
        put: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["ClusterSpec"];
                };
            };
            responses: {
                /** @description 保存后的集群定义，含服务端填写的 created_at；secret_key 原样回传（保存的是全量定义）。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            cluster: components["schemas"]["ClusterSpec"];
                        };
                    };
                };
                /**
                 * @description validate_spec 任一校验失败（集群名称长度不合法 / 名称只能包含字母数字连字符下划线 /
                 *     节点数不足 / 节点列表存在重复 / 镜像标签不合法 / 不接受 latest 标签 /
                 *     标签应形如 RELEASE.<时间戳> 或具体版本号 / 必须给出集群对外服务地址 /
                 *     访问凭据不合法：密钥至少 16 个字符），或保存失败时返回对应 DB 错误信息。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/storage/plans": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 生成部署计划
         * @description 计划精确列出将被清空的每一块设备及其计划时的容量，并带 fingerprint 指纹与 5 分钟有效期
         *     （expires = now()+300）。本步只把「将要发生什么」固化下来，不写任何节点。
         *     生成时逐一校验集群定义存在、节点存在 / 未撤销 / 在线 / 已盘点 / 有可用盘、各节点盘数一致。
         *     成功返回 200 + plan。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["ClusterRefRequest"];
                };
            };
            responses: {
                /** @description 生成的部署计划。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            plan: components["schemas"]["Plan"];
                        };
                    };
                };
                /**
                 * @description 正文为对应中文消息：集群定义不存在 / 节点不存在：<id> / 节点已撤销：<id> /
                 *     节点离线，不能纳入计划：<id> / 节点尚未盘点，不能纳入计划：<id> /
                 *     节点没有可用磁盘，不能纳入计划：<name> /
                 *     各节点可用磁盘数量不一致（<数量列表>），请先补齐再部署 / 保存计划失败（DB 错误信息）。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
                /** @description 盘点缓存读锁失败，正文 {"error":"盘点缓存不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/storage/plans/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 计划 id。 */
                id: string;
            };
            cookie?: never;
        };
        /**
         * 读取部署计划
         * @description 返回计划全文与是否已过期（expired = plan.expires <= now()）。只读 storage_plans 桶。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 计划 id。 */
                    id: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 计划全文与过期标志。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": {
                            /** @description 计划是否已过期（expires <= now()）。 */
                            expired: boolean;
                            plan: components["schemas"]["Plan"];
                        };
                    };
                };
                /** @description 按 id 查不到计划时返回「计划不存在」。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/storage/plans/apply": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 执行部署计划
         * @description 逐节点串行执行：每节点按「磁盘准备 → 容器部署」两步各下发一个任务（各生成一个 task_id）。
         *     任一节点任一步失败即记录到 plan.failed（写审计「已阻断后续节点」）并阻断后续节点，整单返回 400。
         *     执行前按节点实时盘点重新核对：节点离线、盘点丢失、设备消失、容量变化、设备不再是无使用痕迹状态、
         *     指纹不一致，任何一项不符即整单拒绝。破坏性计划（destructive=true）必须显式确认
         *     acknowledge_destructive=true 才能执行。全部成功时写含「不可逆磁盘格式化」的审计。
         *     返回 200（不是 202）与本次下发清单。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["StoragePlanApplyRequest"];
                };
            };
            responses: {
                /** @description 本次执行的计划 id、每个节点每个已下发步骤的记录与已完整执行完成的节点列表。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["StoragePlanApplyResult"];
                    };
                };
                /**
                 * @description 执行前校验或执行中失败，正文为对应中文消息：计划不存在 / 集群定义已不存在，请重新生成计划 /
                 *     计划已过期，请重新生成 / 该计划包含不可逆的磁盘格式化，必须显式确认后才能执行 /
                 *     计划已有失败节点，不能继续执行 / 节点 <name> 已离线，计划作废 /
                 *     节点 <name> 的盘点结果已丢失，计划作废 / 设备 <path> 在 <name> 上已不存在，计划作废 /
                 *     设备 <path> 容量已变化（计划 <n> 字节，实际 <n> 字节），计划作废 /
                 *     设备 <path> 已不再是无使用痕迹的状态，计划作废 / 计划的设备清单已被修改，请重新生成 /
                 *     节点 <name> 执行失败，已阻断后续节点：<reason>。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
                /** @description 盘点缓存读锁失败，正文 {"error":"盘点缓存不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/storage/readiness": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 存储预检报告
         * @description 结论式报告：逐节点给出适合等级与原因、一致性检查、推荐拓扑，以及总体是否可直接进入部署计划。
         *     只读：读 DB 里节点、在线通道与内存盘点缓存，不改库、不下发任务、不广播事件。
         *     未采集的节点标为「待采集」，不会被当作不适合。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 预检报告。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Report"];
                    };
                };
                401: components["responses"]["Unauthorized"];
                /** @description 内部盘点缓存读锁失败，正文 {"error":"盘点缓存不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/storage/status": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 触发集群状态采集
         * @description 登记 StorageStatus 任务并下发给集群中首个在线节点（通道不可用即 400）。
         *     成功返回 200 + task_id（不是 202），label 固定为「状态」。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["ClusterRefRequest"];
                };
            };
            responses: {
                /** @description 已下发的状态采集任务。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["StorageTaskAccepted"];
                    };
                };
                /** @description 正文为对应中文消息：集群不存在 / 集群没有在线节点 / 节点通道不可用 / 任务入库失败（DB 错误信息）。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/storage/users": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 创建或删除集群访问用户
         * @description 用户是集群级操作，下发到任一在线节点执行，不逐节点下发。remove=true 时删除用户，
         *     policy 缺省 readwrite。成功返回 200 + task_id（不是 202），label 固定为「用户」。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["UserRequest"];
                };
            };
            responses: {
                /** @description 已下发的用户任务。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["StorageTaskAccepted"];
                    };
                };
                /** @description 正文为对应中文消息：集群不存在 / 集群没有在线节点 / 节点通道不可用 / 任务入库失败（DB 错误信息）。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tasks": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 任务列表
         * @description 按创建时间倒序。参数正文中的 StackCreate / StackPlan 配置正文已被替换为「[配置正文已隐藏]」。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 任务数组。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["TaskEnvelope"][];
                    };
                };
                400: components["responses"]["BadRequest"];
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tasks/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 任务 UUID。 */
                id: components["parameters"]["TaskId"];
            };
            cookie?: never;
        };
        /** 任务详情 */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 任务 UUID。 */
                    id: components["parameters"]["TaskId"];
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 任务。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["TaskEnvelope"];
                    };
                };
                /** @description 查无此任务时也返回 400「任务不存在」，历史原因不是 404。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/tasks/{id}/events": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 任务 UUID。 */
                id: components["parameters"]["TaskId"];
            };
            cookie?: never;
        };
        /**
         * 任务事件
         * @description 按事件序号升序返回，无分页、无数量上限；任务不存在时返回空数组。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 任务 UUID。 */
                    id: components["parameters"]["TaskId"];
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 事件数组。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["TaskEvent"][];
                    };
                };
                400: components["responses"]["BadRequest"];
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/themes": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 列出已安装主题
         * @description 返回全部已安装主题（含配置项声明），按 short 字典序排列；启用状态见 /api/v1/themes/active。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 已安装主题列表。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ThemeList"];
                    };
                };
                401: components["responses"]["Unauthorized"];
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/themes/{short}": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 主题唯一标识，仅大小写字母/数字/下划线/连字符，长度 1-48；default 保留给内置主题。 */
                short: string;
            };
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /**
         * 删除主题
         * @description 删除主题记录、该主题已保存的设置（theme_settings_<short>）与资源目录；删除的正好是启用中的
         *     主题时同时清除启用记录、回落到内置主题。内置主题 default（忽略大小写）保留不可删。
         */
        delete: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 主题唯一标识，仅大小写字母/数字/下划线/连字符，长度 1-48；default 保留给内置主题。 */
                    short: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 已删除。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Ok"];
                    };
                };
                /** @description short 为 default（忽略大小写，「内置主题不可删除」）；或未安装该主题（「主题不存在」）。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
            };
        };
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/themes/active": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 读取两个界面的启用主题
         * @description 返回控制台与分享页当前启用主题的 describe 输出；未启用时回落到内置主题形态。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 两个界面各自的启用主题描述。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ThemeActive"];
                    };
                };
                401: components["responses"]["Unauthorized"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        /**
         * 切换启用主题
         * @description 按界面写入启用记录与设置；short 为 default（大小写不敏感）时清除启用记录回到内置主题。
         *     无幂等键要求，重复提交以新值覆盖；设置合并声明默认值后编码为 JSON 字符串的长度不得超过
         *     MAX_SETTINGS_BYTES（32 KiB）。
         */
        put: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["ThemeActivateRequest"];
                };
            };
            responses: {
                /** @description 已生效的主题与设置。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ThemeActivateResult"];
                    };
                };
                /**
                 * @description surface 不是 console 或 share（「未知界面：<x>」）；short 非 default 且未安装（「主题未安装」）；
                 *     主题未声明覆盖目标界面（「该主题未声明支持这个界面」）；合并默认值后的设置编码超过
                 *     MAX_SETTINGS_BYTES（32 KiB，「主题设置体积过大」）。这是 400 而不是 404。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/themes/console": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 安装控制台主题（令牌级）
         * @description 清单直接以 JSON 提交，不携带资源文件；校验通过后入库，digest 为主控对序列化清单计算的 sha256。
         *     short 相同时覆盖既有记录（无幂等键）。short 只能含大小写字母/数字/下划线/连字符（1-48 字符），
         *     不得为 default（大小写不敏感）。整个路由处于控制台面默认 1 MiB 请求体上限之下。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["ThemeManifest"];
                };
            };
            responses: {
                /** @description 已安装的主题 short。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ThemeInstallConsoleResult"];
                    };
                };
                /**
                 * @description 清单校验失败（short 为空/超 48 字符/含非法字符/为 default、surfaces 为空、控制台主题缺
                 *     tokens、令牌名不以 -- 开头或令牌值含不允许的字符与写法、configuration.type 非 managed、
                 *     配置项 key 或类型非法，正文为具体校验消息）；或清单未声明支持控制台
                 *     （「该主题未声明支持控制台」）。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                413: components["responses"]["PayloadTooLarge"];
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/themes/share": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 安装分享页主题包（zip 原始字节）
         * @description 请求体为主题包 zip 的原始字节，非 multipart，也不解析 Content-Type，直接读字节流；
         *     包根目录必须含 theme.json 与 index.html。先解压到临时目录、校验通过后再就位，
         *     同 short 覆盖旧记录与资源目录（无幂等键）。包体积上限 MAX_PACKAGE_BYTES（20 MiB），
         *     解压后总量上限 MAX_EXTRACTED_BYTES（64 MiB，防解压炸弹）。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            /** @description 主题包 zip 的原始字节。 */
            requestBody: {
                content: {
                    "application/octet-stream": string;
                };
            };
            responses: {
                /** @description 已安装的主题 short 与内容摘要。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ThemeInstallShareResult"];
                    };
                };
                /**
                 * @description 请求体为空（「请求体为空」）、主题包不是有效的 zip（「主题包不是有效的 zip」）、根目录缺
                 *     theme.json、theme.json 解析失败、清单校验失败、清单未声明支持分享页、根目录缺 index.html、
                 *     包内越界路径、解压后总大小超 64 MiB 上限。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                401: components["responses"]["Unauthorized"];
                403: components["responses"]["OriginOrCsrfRejected"];
                /** @description 请求体超过 MAX_PACKAGE_BYTES（20 MiB），由路由层 DefaultBodyLimit 在进入处理函数前拒绝；正文是 axum 默认纯文本，不是本模块的 JSON 错误体。 */
                413: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "text/plain": "Failed to buffer the request body";
                    };
                };
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/renew": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * Agent 证书续期
         * @description Agent mTLS 面（独立监听、要求客户端证书），不属于浏览器接口。身份由证书解析，不改节点状态、不产生任务。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["RenewRequest"];
                };
            };
            responses: {
                /** @description 续期后的客户端证书。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["RenewResponse"];
                    };
                };
                /** @description 身份校验失败（无证书、未登记、已过期、节点已撤销）或 CSR 无效。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 分享令牌明文。门禁按摘要常量时间校验，无效令牌与错误入口一样被直接丢弃连接、不产生任何 HTTP 响应。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 分享页首页
         * @description 返回分享页 HTML（text/html; charset=utf-8）。启用了包级分享主题时返回主题入口 HTML 并带独立 CSP 头，
         *     否则回落内置 share.html。只读，不写库、不广播、不产生任务；不设置缓存响应头。
         *     带结尾斜杠的 /share/{token}/ 是分享面入口；不带斜杠的 /share/{token} 会先被 301 永久重定向到此处。
         *     令牌无效时门禁直接丢弃连接、没有常规 HTTP 响应，因此不会出现 401 或 404 JSON。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 分享令牌明文。门禁按摘要常量时间校验，无效令牌与错误入口一样被直接丢弃连接、不产生任何 HTTP 响应。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 分享页 HTML 正文。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "text/html": string;
                    };
                };
                /**
                 * @description 纯文本「分享页尚未构建」（无 JSON error 包装）：读取 s.web/share.html 失败，即内置页未构建且未启用分享主题。
                 *     令牌无效时门禁会直接丢弃连接、没有常规 HTTP 响应，不会到达本处理函数。
                 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "text/plain": "分享页尚未构建";
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/api/v1/public/metrics": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 分享令牌明文（限定可见节点范围）。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 公开 Prometheus 指标（机器接口）
         * @description 机器接口，双授权：分享令牌同 Authorization: Bearer <API Key>，节点范围取交集。
         *     返回 Prometheus 文本格式平文（Content-Type: text/plain; version=0.0.4; charset=utf-8），
         *     每条样本只带 node 与 display_name 两个标签，**不输出地址**；缺失的采样不输出样本行，
         *     面板上表现为断档而不是被记成 0。只读，可能写回 API Key 的 lastUsed；不设置缓存响应头。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 分享令牌明文（限定可见节点范围）。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /**
                 * @description Prometheus 文本格式平文（# HELP / # TYPE 与样本行），Content-Type 严格为 text/plain; version=0.0.4; charset=utf-8。
                 *     指标为 opsd_node_online、opsd_node_cpu_usage_percent、opsd_node_memory_used_bytes、opsd_node_memory_total_bytes、
                 *     opsd_node_disk_used_bytes、opsd_node_disk_total_bytes、opsd_node_load1、
                 *     opsd_node_network_receive_bytes_per_second、opsd_node_network_transmit_bytes_per_second、opsd_node_uptime_seconds。
                 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "text/plain": string;
                    };
                };
                /** @description 分享页已关闭，正文 {"error":"分享页已关闭"}（JSON，不是 Prometheus 文本）。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 缺少 Authorization: Bearer 头，正文 {"error":"缺少 API Key"}；或密钥摘要未匹配/已撤销，正文 {"error":"API Key 无效"}。 */
                401: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 令牌不在分享索引中，正文 {"error":"分享链接无效或已失效"}。 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 分享索引读写锁中毒，正文 {"error":"分享索引不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/api/v1/public/nodes": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 分享令牌明文（限定可见节点范围）。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 公开节点列表（机器接口）
         * @description 机器接口（application/json），双授权：分享令牌同 Authorization: Bearer <API Key>，节点范围取交集。
         *     只返回可达交集内各节点详情；字段与页面接口同一套公开白名单（PublicNode），刻意不含内部地址、资源细节与凭据。
         *     只读，可能写回 API Key 的 lastUsed。不设置缓存响应头。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 分享令牌明文（限定可见节点范围）。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 交集后可访问节点数组。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareNodes"];
                    };
                };
                /** @description 分享页已关闭，正文 {"error":"分享页已关闭"}。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 缺少 Authorization: Bearer 头，正文 {"error":"缺少 API Key"}；或密钥摘要未匹配/已撤销，正文 {"error":"API Key 无效"}。 */
                401: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 令牌不在分享索引中，正文 {"error":"分享链接无效或已失效"}。 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 分享索引读写锁中毒，正文 {"error":"分享索引不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/api/v1/public/recent/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 节点编号，须同时落在令牌范围与 API Key 节点范围内。 */
                id: string;
                /** @description 分享令牌明文（限定可见节点范围）。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 单节点最近采样（机器接口）
         * @description 机器接口（application/json），双授权：分享令牌同 Authorization: Bearer <API Key>，节点范围取交集。
         *     返回该节点最近一次成功采样折算的公开结构（字段同 PublicNode）：无采样或最近上报失败时，指标字段与
         *     metrics_at 都为 null 而不是 0。刻意不含内部地址、资源细节与凭据。只读，可能写回 API Key 的 lastUsed；
         *     不设置缓存响应头。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 节点编号，须同时落在令牌范围与 API Key 节点范围内。 */
                    id: string;
                    /** @description 分享令牌明文（限定可见节点范围）。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 单个节点的公开结构与最近采样时刻。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareRecent"];
                    };
                };
                /**
                 * @description 节点不在 API Key 与令牌的交集范围内，正文 {"error":"该节点不在访问范围内"}；
                 *     或节点已撤销/被隐藏/不在令牌范围内，正文 {"error":"节点不可见"}。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 缺少 Authorization: Bearer 头，正文 {"error":"缺少 API Key"}；或密钥摘要未匹配/已撤销，正文 {"error":"API Key 无效"}。 */
                401: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 令牌不在分享索引中，正文 {"error":"分享链接无效或已失效"}。 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 分享索引读写锁中毒，正文 {"error":"分享索引不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/api/v1/public/records": {
        parameters: {
            query: {
                /** @description 起始 Unix 秒。 */
                from: number;
                /** @description 节点编号，须同时落在令牌范围与 API Key 节点范围内。 */
                node: string;
                /** @description 步长（秒），默认 300，运行时 clamp 到 [60, 86400]。 */
                step?: number;
                /** @description 结束 Unix 秒，必须晚于 from。 */
                to: number;
            };
            header?: never;
            path: {
                /** @description 分享令牌明文（限定可见节点范围）。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 公开指标历史降采样（机器接口）
         * @description 机器接口（application/json），双授权：分享令牌同 Authorization: Bearer <API Key>，节点范围取交集。
         *     双重鉴权通过后复用页面接口的降采样逻辑，故 200 响应体与 /share/{token}/data/records 完全一致，
         *     但拒绝文案不同：节点不在访问范围内时是 {"error":"该节点不在访问范围内"}。
         *     点数上限 2000，缺失区间自然断开、不插值不补零；只读，可能写回 API Key 的 lastUsed；不设置缓存响应头。
         */
        get: {
            parameters: {
                query: {
                    /** @description 起始 Unix 秒。 */
                    from: number;
                    /** @description 节点编号，须同时落在令牌范围与 API Key 节点范围内。 */
                    node: string;
                    /** @description 步长（秒），默认 300，运行时 clamp 到 [60, 86400]。 */
                    step?: number;
                    /** @description 结束 Unix 秒，必须晚于 from。 */
                    to: number;
                };
                header?: never;
                path: {
                    /** @description 分享令牌明文（限定可见节点范围）。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 节点编号、选用层级、实际步长与降采样曲线点，与页面接口 /data/records 同形。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareRecords"];
                    };
                };
                /**
                 * @description 分享页已关闭，正文 {"error":"分享页已关闭"}；节点不在 API Key 与令牌的交集范围内，正文 {"error":"该节点不在访问范围内"}；
                 *     或复用逻辑返回的 {"error":"该节点不在分享范围内"}；to<=from，正文 {"error":"结束时间必须晚于开始时间"}；
                 *     或 (to-from)/step 超过 2000 点，正文 {"error":"区间与步长组合返回点数过多"}。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 缺少 Authorization: Bearer 头，正文 {"error":"缺少 API Key"}；或密钥摘要未匹配/已撤销，正文 {"error":"API Key 无效"}。 */
                401: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 令牌不在分享索引中，正文 {"error":"分享链接无效或已失效"}。 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 分享索引读写锁中毒，正文 {"error":"分享索引不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/api/v1/public/summary": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 分享令牌明文（限定可见节点范围）。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 公开节点概况（机器接口）
         * @description 机器接口（application/json），双授权：分享令牌放行 **加上** Authorization: Bearer <API Key>，
         *     可见节点范围为令牌范围与 API Key 节点范围的交集。只读，仅可能写回 key.last_used（距上次更新超 60 秒时）。
         *     API Key 经摘要校验；响应只含交集内的计数，不含节点地址、资源细节与凭据。不设置缓存响应头，不应被代理缓存。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 分享令牌明文（限定可见节点范围）。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 交集可见节点计数，量值不经缓存，实时读取。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareSummaryBase"];
                    };
                };
                /** @description 分享页已关闭，正文 {"error":"分享页已关闭"}。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 缺少 Authorization: Bearer 头，正文 {"error":"缺少 API Key"}；或密钥摘要未匹配/已撤销，正文 {"error":"API Key 无效"}。 */
                401: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 令牌不在分享索引中，正文 {"error":"分享链接无效或已失效"}。 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 分享索引读写锁中毒，正文 {"error":"分享索引不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/assets/{path}": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 构建产物 assets 目录内的相对路径，可含子目录；读取前做防目录穿越校验。 */
                path: string;
                /** @description 分享令牌明文。实际不会被处理函数使用（路径提取被忽略），门禁已预先校验。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 分享页构建产物静态资源
         * @description 只读磁盘文件，返回 assets 目录下的静态文件二进制，Content-Type 依扩展名：
         *     js→text/javascript、css→text/css、svg→image/svg+xml、json→application/json、woff2→font/woff2、其他→application/octet-stream。
         *     不设置缓存响应头；响应体是构建产物原文，绝不包含令牌、凭据或服务端路径信息。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 构建产物 assets 目录内的相对路径，可含子目录；读取前做防目录穿越校验。 */
                    path: string;
                    /** @description 分享令牌明文。实际不会被处理函数使用（路径提取被忽略），门禁已预先校验。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 文件二进制，Content-Type 依扩展名而定（见操作描述）。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/octet-stream": string;
                    };
                };
                /** @description 空正文（无 JSON、无文本）：path 包含 ".." 或以 / 开头（防目录穿越），或目标文件读取失败。 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/data/nodes": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 分享令牌明文。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 分享页可见节点列表
         * @description 页面接口（application/json），仅凭分享令牌放行。只返回公开白名单字段（见 PublicNode），刻意不包含内部地址、资源细节与凭据；
         *     节点为未撤销、未隐藏且在令牌范围内者，按权重降序、再按名称升序。只读，不设置缓存响应头。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 分享令牌明文。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 可见节点数组。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareNodes"];
                    };
                };
                /** @description 分享页已关闭，正文 {"error":"分享页已关闭"}。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 令牌不在分享索引中，正文 {"error":"分享链接无效或已失效"}。 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 分享索引读写锁中毒，正文 {"error":"分享索引不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/data/records": {
        parameters: {
            query: {
                /** @description 起始 Unix 秒。 */
                from: number;
                /** @description 节点编号，须在令牌可见范围内。 */
                node: string;
                /** @description 步长（秒），默认 300，运行时 clamp 到 [60, 86400]。 */
                step?: number;
                /** @description 结束 Unix 秒，必须晚于 from。 */
                to: number;
            };
            header?: never;
            path: {
                /** @description 分享令牌明文。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 分享页指标历史降采样
         * @description 页面接口（application/json），仅凭分享令牌放行。只读查指标历史，不写库、不广播；不设置缓存响应头。
         *     点数上限 2000；缺失区间自然断开，不插值不补零。响应刻意不含节点地址、资源细节与凭据。
         */
        get: {
            parameters: {
                query: {
                    /** @description 起始 Unix 秒。 */
                    from: number;
                    /** @description 节点编号，须在令牌可见范围内。 */
                    node: string;
                    /** @description 步长（秒），默认 300，运行时 clamp 到 [60, 86400]。 */
                    step?: number;
                    /** @description 结束 Unix 秒，必须晚于 from。 */
                    to: number;
                };
                header?: never;
                path: {
                    /** @description 分享令牌明文。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 回显节点编号、选用层级、实际步长与降采样曲线点。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareRecords"];
                    };
                };
                /**
                 * @description 分享页已关闭，正文 {"error":"分享页已关闭"}；节点不在令牌范围，正文 {"error":"该节点不在分享范围内"}；
                 *     to<=from，正文 {"error":"结束时间必须晚于开始时间"}；或 (to-from)/step 超过 2000 点，正文 {"error":"区间与步长组合返回点数过多"}。
                 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 令牌不在分享索引中，正文 {"error":"分享链接无效或已失效"}。 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 分享索引读写锁中毒，正文 {"error":"分享索引不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/data/summary": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 分享令牌明文。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 分享页站点与节点概况
         * @description 页面接口（application/json），仅凭分享令牌放行，不需要 Cookie 或 API Key。只读，不写库、不广播；
         *     site 读取失败时回落到 SitePublic 默认值。不设置缓存响应头，不应被代理缓存。
         *     响应刻意只含站点公开属性与计数，不含任何内部地址、资源细节与凭据。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 分享令牌明文。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 站点公开属性与令牌可见节点计数。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ShareSummary"];
                    };
                };
                /** @description 分享页已关闭（ensure_enabled 校验），正文 {"error":"分享页已关闭"}；错误统一由 ApiError 包装为 JSON。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 令牌不在分享索引中，正文 {"error":"分享链接无效或已失效"}。 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
                /** @description 分享索引读写锁中毒，正文 {"error":"分享索引不可用"}。 */
                500: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                    };
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/data/theme": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 分享令牌明文，门禁按 sha256 摘要查找。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 读取分享页公开主题数据
         * @description 返回分享页当前启用主题的 describe 输出：short、名称、令牌、公开设置与配置项声明。
         *     设置与配置项声明公开可读，不得放入密钥；未启用包级主题时同样返回内置主题形态
         *     （short 为 default），不报错。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 分享令牌明文，门禁按 sha256 摘要查找。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 分享页主题公开数据。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["ThemeDescribe"];
                    };
                };
                /** @description 令牌无效、过期或被撤销（门禁直接丢弃，不产生业务响应）。空正文。 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/share/{token}/theme/{path}": {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 主题资源相对路径（资源目录内），可含子路径。 */
                path: string;
                /** @description 分享令牌明文，门禁按 sha256 摘要查找。 */
                token: string;
            };
            cookie?: never;
        };
        /**
         * 读取分享页主题资源
         * @description 返回主题资源原始字节。Content-Type 按扩展名推测：js/mjs→text/javascript、css→text/css、
         *     html→text/html; charset=utf-8、svg→image/svg+xml、json→application/json、png→image/png、
         *     jpg/jpeg→image/jpeg、webp→image/webp、woff2→font/woff2，其余→application/octet-stream；
         *     并附独立 CSP 响应头（default-src 'none'，只放行 self，内联样式例外）。路径必须落在该主题
         *     资源目录内；未启用包级分享主题时本端点恒 404。
         */
        get: {
            parameters: {
                query?: never;
                header?: never;
                path: {
                    /** @description 主题资源相对路径（资源目录内），可含子路径。 */
                    path: string;
                    /** @description 分享令牌明文，门禁按 sha256 摘要查找。 */
                    token: string;
                };
                cookie?: never;
            };
            requestBody?: never;
            responses: {
                /** @description 主题资源原始字节，带按扩展名推测的 Content-Type 与 CSP 响应头。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/octet-stream": string;
                    };
                };
                /**
                 * @description 令牌无效/过期/被撤销、path 包含 .. 或以 / 开头、当前未启用包级分享主题（has_assets=false）、
                 *     或资源读取失败。空正文。
                 */
                404: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content?: never;
                };
            };
        };
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/verify": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 全新连接验证
         * @description Agent mTLS 面。向 witness 节点下发探测帧并轮询结果，用于在防火墙变更后从一条**全新连接**验证可达性。
         *     最多轮询 6 秒；失败原因写在 400 正文里。
         */
        post: {
            parameters: {
                query?: never;
                header?: never;
                path?: never;
                cookie?: never;
            };
            requestBody: {
                content: {
                    "application/json": components["schemas"]["VerifyRequest"];
                };
            };
            responses: {
                /** @description 验证通过。 */
                200: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["VerifyResponse"];
                    };
                };
                /** @description witness 不可选、验证节点不在线、未登记可探测地址、通道断开、验证失败或超时。 */
                400: {
                    headers: {
                        [name: string]: unknown;
                    };
                    content: {
                        "application/json": components["schemas"]["Error"];
                        "text/plain": string;
                    };
                };
                415: components["responses"]["UnsupportedMediaType"];
                422: components["responses"]["UnprocessableEntity"];
            };
        };
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
export type webhooks = Record<string, never>;
export interface components {
    schemas: {
        /**
         * @description 节点操作负载，按 type 判别，拒绝未知字段。这是控制台唯一的下发手段：
         *     Agent 侧不存在接收任意命令的接口。
         */
        Action: components["schemas"]["ActionInspect"] | components["schemas"]["ActionHostInspect"] | components["schemas"]["ActionDbInspect"] | components["schemas"]["ActionDockerInspect"] | components["schemas"]["ActionDocker"] | components["schemas"]["ActionPullImage"] | components["schemas"]["ActionStackImport"] | components["schemas"]["ActionStackCreate"] | components["schemas"]["ActionStackPlan"] | components["schemas"]["ActionStackApply"] | components["schemas"]["ActionStackControl"] | components["schemas"]["ActionFirewallPlan"] | components["schemas"]["ActionFirewallApply"] | components["schemas"]["ActionPeerSync"] | components["schemas"]["ActionPeerProbeTargets"] | components["schemas"]["ActionFilePut"] | components["schemas"]["ActionFileRemove"] | components["schemas"]["ActionFileRename"] | components["schemas"]["ActionFileMkdir"] | components["schemas"]["ActionFileChmod"] | components["schemas"]["ActionFileChown"] | components["schemas"]["ActionStoragePrepare"] | components["schemas"]["ActionStorageDeploy"] | components["schemas"]["ActionStorageStatus"] | components["schemas"]["ActionStorageBucket"] | components["schemas"]["ActionStorageUser"];
        ActionDbInspect: {
            /** @constant */
            type: "db_inspect";
        };
        ActionDocker: {
            container: string;
            operation: components["schemas"]["DockerOperation"];
            /** @constant */
            type: "docker";
        };
        ActionDockerInspect: {
            container: string;
            /** @constant */
            type: "docker_inspect";
        };
        ActionFileChmod: {
            mode: number;
            path: string;
            /** @constant */
            type: "file_chmod";
        };
        ActionFileChown: {
            gid: number;
            path: string;
            /** @constant */
            type: "file_chown";
            uid: number;
        };
        ActionFileMkdir: {
            path: string;
            /** @constant */
            type: "file_mkdir";
        };
        ActionFilePut: {
            digest: string;
            path: string;
            /** Format: int64 */
            size: number;
            /** @constant */
            type: "file_put";
        };
        ActionFileRemove: {
            /**
             * @description 递归删除必须显式 true，缺省即拒绝。
             * @default false
             */
            confirmed: boolean;
            path: string;
            /** @default false */
            recursive: boolean;
            /** @constant */
            type: "file_remove";
        };
        ActionFileRename: {
            from: string;
            to: string;
            /** @constant */
            type: "file_rename";
        };
        ActionFirewallApply: {
            plan_id: string;
            /** @constant */
            type: "firewall_apply";
            /** @description 验证节点。必须是与本节点不同的在线节点；缺失时自动挑选。 */
            witness?: string | null;
        };
        ActionFirewallPlan: {
            /** @description 防火墙计划负载。源码为自由 JSON，结构由防火墙计划语义决定。 */
            operation: {
                [key: string]: unknown;
            };
            /** @constant */
            type: "firewall_plan";
        };
        ActionHostInspect: {
            /** @constant */
            type: "host_inspect";
        };
        ActionInspect: {
            /** @constant */
            type: "inspect";
        };
        ActionPeerProbeTargets: {
            set: components["schemas"]["ProbeTargetSet"];
            /** @constant */
            type: "peer_probe_targets";
        };
        /** @description 仅由节点目录生成，外部提交会被 400 拒绝。 */
        ActionPeerSync: {
            set: components["schemas"]["PeerAddressSet"];
            /** @constant */
            type: "peer_sync";
        };
        ActionPullImage: {
            reference: string;
            /** @constant */
            type: "pull_image";
        };
        ActionStackApply: {
            plan_id: string;
            /** @constant */
            type: "stack_apply";
        };
        ActionStackControl: {
            operation: components["schemas"]["StackOperation"];
            project: string;
            /** @constant */
            type: "stack_control";
        };
        ActionStackCreate: {
            /** @description Compose 配置正文。在任务列表与详情中会被脱敏为「[配置正文已隐藏]」。 */
            content: string;
            project: string;
            /** @constant */
            type: "stack_create";
        };
        ActionStackImport: {
            directory: string;
            env_files: string[];
            files: string[];
            project: string;
            /** @constant */
            type: "stack_import";
        };
        ActionStackPlan: {
            /** @description Compose 配置正文。在任务列表与详情中会被脱敏为「[配置正文已隐藏]」。 */
            content: string;
            project: string;
            /** @constant */
            type: "stack_plan";
        };
        ActionStorageBucket: {
            bucket: string;
            cluster: string;
            /** @default false */
            remove: boolean;
            /** @constant */
            type: "storage_bucket";
        };
        ActionStorageDeploy: {
            /** @description 访问凭据的键名。凭据本身由 Agent 本地保存，不经过网络。 */
            access_key: string;
            cluster: string;
            /** @description 固定 release 标签，不接受 latest。 */
            image: string;
            /** @description 本节点参与集群的挂载点。 */
            mounts: string[];
            /** @description 集群全部节点地址（含本机）。 */
            peers: string[];
            plan_id: string;
            secret_key: string;
            /** @constant */
            type: "storage_deploy";
        };
        /** @description 全系统唯一会销毁数据的操作：会清空 devices 中列出的每一块盘。 */
        ActionStoragePrepare: {
            cluster: string;
            devices: components["schemas"]["StorageDeviceTarget"][];
            /** @description 文件系统类型，目前只接受 xfs。 */
            filesystem: string;
            /** @description 计划编号，仅用于把执行与计划、审计串起来。 */
            plan_id: string;
            /** @constant */
            type: "storage_prepare";
        };
        ActionStorageStatus: {
            cluster: string;
            /** @constant */
            type: "storage_status";
        };
        ActionStorageUser: {
            cluster: string;
            /** @default readwrite */
            policy: string;
            /** @default false */
            remove: boolean;
            secret: string;
            /** @constant */
            type: "storage_user";
            user: string;
        };
        /** @enum {string} */
        AgentInstallMode: "host" | "docker";
        ApiKey: {
            /**
             * Format: int64
             * @description 创建时间,Unix 秒。
             */
            created_at: number;
            /** @description sha256(明文密钥) 十六进制摘要;明文永不落库、不再出现在任何响应中。 */
            digest: string;
            /** @description 密钥记录 id。 */
            id: string;
            /** @description 密钥名称。 */
            label: string;
            /**
             * Format: int64
             * @description 最近一次机器接口使用时间,Unix 秒;从未使用或较上次写入相差不超过 60 秒时为 null 或旧值(只在相差超过 60 秒时才写回)。
             */
            last_used: number | null;
            /** @description 限定可见节点 id 列表;null 表示全部(授权时与分享令牌范围取交集)。 */
            nodes: string[] | null;
            /** @description 是否已撤销。 */
            revoked: boolean;
        };
        ApiKeyCreated: {
            /** @description 明文密钥(opsd_ 前缀),仅此一次返回,之后无法再取回。 */
            key: string;
            /** @description 密钥记录本身,last_used 为 null。 */
            record: components["schemas"]["ApiKey"];
        };
        /** @description 创建 API 密钥的请求体。拒绝未知字段（serde deny_unknown_fields），与 additionalProperties: false 一致。 */
        ApiKeyInput: {
            /** @description 密钥名称,trim 后非空且不超过 60 字符,否则 400。 */
            label: string;
            /** @description 限定可见节点 id 列表;省略或 null 表示全部。 */
            nodes?: string[] | null;
        };
        ApiKeyList: {
            /** @description 按 created_at 降序;只含摘要与元数据,绝不包含明文密钥(明文仅创建响应一次)。 */
            keys: components["schemas"]["ApiKey"][];
        };
        AuditRecord: {
            actor: string;
            /**
             * Format: int64
             * @description 落库时间（Unix 秒），排序与时间筛选的基准。
             */
            at: number;
            /** Format: int64 */
            bytes: number;
            /** @description 动作类别，例如 会话 / 文件 / 数据库 / 存储 / 防火墙 / 容器 / 分享页 / 主题。 */
            category: string;
            /** @description 补充说明。依约定不含凭据、环境变量、SQL 正文与文件内容。 */
            detail: string;
            /** Format: int64 */
            duration: number;
            id: string;
            node_id: string;
            /** @description 结果语义，例如 已开始 / 已结束 / 成功 / 失败 / 已提交 / 已存在。 */
            result: string;
            /** @description 来源 IP。 */
            source: string;
            target: string;
        };
        BackupState: {
            /**
             * Format: int64
             * @description 上次触发时刻;null 表示未知。
             */
            last_trigger: number | null;
            /**
             * Format: int64
             * @description systemd 定时器下次触发时刻;没有定时器时为 null。
             */
            next_run: number | null;
            /** @description 备份根目录。 */
            root: string;
            /** @description 各备份层级现状,可为空数组。 */
            tiers: components["schemas"]["BackupTier"][];
        };
        /** @description 单个备份层级现状。 */
        BackupTier: {
            /** @description 归档是否为 age 格式(只检查文件头);null 表示无法判断。 */
            age_header_ok: boolean | null;
            /**
             * Format: int64
             * @description 最近一次成功距今秒数,新鲜度判定依据;hourly 阈值 5400 秒(90 分钟),daily 阈值 93600 秒(26 小时)。
             */
            age_seconds: number | null;
            /** @description 该层级的备份目录。 */
            directory: string;
            /** @description 目录里归档文件个数。 */
            files: number;
            /** @description 是否存在 SHA256SUMS 校验清单。 */
            has_checksums: boolean;
            /**
             * Format: int64
             * @description 最近一次成功的时刻;null 表示没有成功记录,判为未知而非过期。
             */
            last_success: number | null;
            /**
             * Format: int64
             * @description 最近一次成功那份备份的体积;null 表示未知。
             */
            size_bytes: number | null;
            /** @description hourly / daily / weekly。 */
            tier: string;
        };
        BucketRequest: {
            /** @description 桶名。 */
            bucket: string;
            /** @description 集群定义名称。 */
            cluster: string;
            /**
             * @description true 时删除桶而非创建，缺省 false。
             * @default false
             */
            remove: boolean;
        };
        Candidate: {
            /** @description 设备名。 */
            device: string;
            /** @description 磁盘型号，可缺失或为 null。 */
            model?: string | null;
            /** @description 设备路径。 */
            path: string;
            /** @description 是否机械盘。 */
            rotational: boolean;
            /**
             * Format: int64
             * @description 容量字节。
             */
            size: number;
        };
        Check: {
            /** @description 不通过时的具体说明。 */
            detail: string;
            /** @description 检查项名。 */
            name: string;
            /** @description 是否通过。 */
            passed: boolean;
        };
        ClusterListItem: {
            /** @description 访问键名。 */
            access_key: string;
            /**
             * Format: int64
             * @description 服务端填写的创建时间。
             */
            created_at?: number;
            /** @description 对外服务地址。 */
            endpoint: string;
            /** @description 密钥是否已设置（secret_key 非空时为 true）；secret_key 一律不回传。 */
            has_secret_key: boolean;
            /** @description 固定 release 标签。 */
            image_tag: string;
            /** @description 集群名称。 */
            name: string;
            /** @description 参与集群的节点编号。 */
            nodes: string[];
        };
        ClusterRefRequest: {
            /** @description 集群定义名称。 */
            name: string;
        };
        ClusterSpec: {
            /** @description 访问键名，至少 3 字符。 */
            access_key: string;
            /**
             * Format: int64
             * @description 由服务端填写：新建时设为当前时间，已存在则保留旧值；客户端可不传。
             */
            created_at?: number;
            /** @description 对外服务地址，非空，用于生成分布式拓扑。 */
            endpoint: string;
            /** @description 固定 release 标签，不接受 latest；应形如 RELEASE.<时间戳> 或以数字开头，最长 80。 */
            image_tag: string;
            /** @description 集群名称，1-40 字符，仅字母、数字、连字符与下划线。 */
            name: string;
            /** @description 参与集群的节点编号，至少 4 个且不重复。 */
            nodes: string[];
            /** @description 访问键密钥，非空且至少 16 字符。 */
            secret_key: string;
        };
        /** @description SSE 帧的 data 载荷，按 type 判别。 */
        ConsoleEvent: {
            task?: components["schemas"]["TaskEnvelope"];
            /** @enum {string} */
            type: "tasks" | "nodes" | "metrics" | "storage" | "database" | "task" | "resync";
        } & {
            [key: string]: unknown;
        };
        DatabaseNodeVerdict: {
            /**
             * Format: int64
             * @description 最近一次巡检时刻(Unix 秒);null 表示从未巡检过,这不是 0。
             */
            collected_at: number | null;
            /** @description Agent 本机是否配置了只读巡检账号;只是布尔标记,不含任何凭据本身。 */
            configured: boolean;
            /** @description 已确定的问题列表,可为空数组。 */
            findings: components["schemas"]["Finding"][];
            /** @description 节点显示名。 */
            name: string;
            /** @description 节点 ID。 */
            node_id: string;
            /** @description 节点当前是否在线。 */
            online: boolean;
            /** @description 原始巡检报告全文,供界面展开细节;null 表示没有报告。 */
            report: components["schemas"]["DbInspectReport"] | null;
            tone: components["schemas"]["Tone"];
            /** @description 未知项原因列表(尚未巡检/当前离线/未配置账号/某分区采集失败等),中文;界面必须展示,不能只显示 tone。 */
            unknown: string[];
        };
        /** @description 数据库主机侧分区采集到的事实。 */
        DbHostState: {
            /** @description binlog 文件名;未开启 binlog 时为 null,这是「没有」不是错误。 */
            binlog_file?: string | null;
            /** @description binlog 位点;null 表示未知。 */
            binlog_position?: number | null;
            /** @description 主机名;null 表示未知。 */
            hostname?: string | null;
            /** @description read_only 是否 ON;null 为未知。 */
            read_only?: boolean | null;
            /** @description server_id;null 表示未知。 */
            server_id?: number | null;
            /** @description 已连接线程数;null 表示未知。 */
            threads_connected?: number | null;
            /** @description 运行中线程数;null 表示未知。 */
            threads_running?: number | null;
            /**
             * Format: int64
             * @description 运行秒数;null 表示未知。
             */
            uptime?: number | null;
            /** @description 数据库版本字符串。 */
            version: string;
            /** @description 版本注释。 */
            version_comment: string;
        };
        /** @description 原始巡检报告全文。Agent 只带回事实,阈值与比较在控制台完成;不含任何凭据类字段。 */
        DbInspectReport: {
            backup: components["schemas"]["DbSection"];
            /**
             * Format: int64
             * @description 采集时刻(Unix 秒)。
             */
            collected_at: number;
            /** @description Agent 本机是否配置了只读巡检账号;未配置时各分区 data 为 null 并带 reason。 */
            configured: boolean;
            galera: components["schemas"]["DbSection"];
            /** @description 巡检过程中被跳过的部分与原因,可为空数组。 */
            gaps: string[];
            host: components["schemas"]["DbSection"];
            proxysql: components["schemas"]["DbSection"];
        };
        /** @description 只读巡检请求体,只接受幂等键一个字段(动作固定为 db_inspect,无动作负载)。 */
        DbInspectRequest: {
            /** @description 幂等键。同节点 + 同键复用既有任务并返回其 task_id;同键但参数摘要不同则 400 拒绝。 */
            idempotency_key: string;
        };
        /**
         * @description 分区式采集结果。data 为 null 即该分区未知(连不上/没权限/未配置)——这是「未知不等于 0」
         *     的表达:不是缺字段、不是 0、不是 "unknown" 枚举,而是 data 为 null 且 reason 给出原因。
         */
        DbSection: {
            /** @description 采集到的事实;null 表示该分区未知,绝不补 0。 */
            data: components["schemas"]["GaleraState"] | components["schemas"]["DbHostState"] | components["schemas"]["ProxySqlState"] | components["schemas"]["BackupState"] | null;
            /** @description 采集失败或未配置时的具体原因(中文);已知时此值为 null。 */
            reason: string | null;
        };
        /** @description 真实挂载点的容量与 inode 用量。 */
        DiskUsage: {
            /** @description 文件系统类型；底层为 /proc/mounts 的对应字段，块设备写入设备路径。 */
            filesystem: string;
            /**
             * Format: int64
             * @description inode 总数。
             */
            inode_total: number;
            /**
             * Format: int64
             * @description 已用 inode 数。
             */
            inode_used: number;
            /** @description 挂载点。 */
            mount: string;
            /**
             * Format: int64
             * @description 容量（字节）。
             */
            total: number;
            /**
             * Format: int64
             * @description 已用（字节）。
             */
            used: number;
        };
        DispatchRecord: {
            /** @description 节点名。 */
            node: string;
            /**
             * @description 步骤名：先准备磁盘，成功后再部署容器。
             * @enum {string}
             */
            step: "磁盘准备" | "容器部署";
            /** @description 该步骤对应任务 id，可在任务面板追溯。 */
            task_id: string;
        };
        /** @enum {string} */
        DockerOperation: "start" | "stop" | "restart";
        EnrollmentSummary: {
            /** Format: int64 */
            created_at: number;
            /** Format: int64 */
            expires_at: number;
            install_mode: components["schemas"]["AgentInstallMode"];
            name: string;
            node_id: string;
            overlay_address?: string | null;
            public_addresses: string[];
            ssh_port: number;
            /** @enum {string} */
            status: "pending" | "expired";
        };
        EnrollmentToken: {
            /** @description Agent 镜像引用；部署时应替换为固定 digest。 */
            agent_image: string;
            /**
             * Format: uri
             * @description Agent mTLS WebSocket 地址，路径固定为 /agent。
             */
            agent_url: string;
            /** @description CA 证书 SHA-256 指纹（hex），供 Agent 核对。 */
            ca_fingerprint: string;
            /** Format: int64 */
            expires_at: number;
            /** @constant */
            expires_in: 600;
            install_mode: components["schemas"]["AgentInstallMode"];
            node_id: string;
            /** @description 注册令牌明文，一次性使用，仅在本次响应中出现。 */
            token: string;
        };
        EnrollRequest: {
            /** @description PEM 编码的 CSR。 */
            csr: string;
            /** @description 一次性注册令牌。 */
            token: string;
        };
        EnrollResponse: {
            /** @description 签名后的客户端证书 PEM，有效期 90 天。 */
            certificate: string;
            node_id: string;
        };
        EntranceUpdate: {
            /** @description 与当前入口一致时为 false。 */
            changed: boolean;
            value: string;
        };
        EntranceValue: {
            /** @description 安全入口字符串。写入时必须 16 位，仅字母数字与 -._~，且不以 . 或 - 开头。 */
            value: string;
        };
        Error: {
            /** @description 中文错误消息。不含凭据、环境变量、SQL 正文与文件内容。 */
            error: string;
        };
        /**
         * @description 结构化文件操作，按 type 判别，拒绝未知字段。
         *     各变体的字段都是必填（只有 remove 的 recursive / confirmed 有默认值），与 Action 的文件类变体同形；
         *     缺字段会被框架在进入处理函数前拒绝（422 纯文本）。
         */
        FileRequest: components["schemas"]["FileRequestPut"] | components["schemas"]["FileRequestRemove"] | components["schemas"]["FileRequestRename"] | components["schemas"]["FileRequestMkdir"] | components["schemas"]["FileRequestChmod"] | components["schemas"]["FileRequestChown"];
        FileRequestChmod: {
            /** @description 权限位。 */
            mode: number;
            path: string;
            /** @constant */
            type: "chmod";
        };
        FileRequestChown: {
            gid: number;
            path: string;
            /** @constant */
            type: "chown";
            uid: number;
        };
        FileRequestMkdir: {
            path: string;
            /** @constant */
            type: "mkdir";
        };
        FileRequestPut: {
            /** @description 只有暂存内容摘要与它一致才会覆盖目标文件。 */
            digest: string;
            path: string;
            /** Format: int64 */
            size: number;
            /** @constant */
            type: "put";
        };
        FileRequestRemove: {
            /**
             * @description 递归删除必须显式 true，缺省即拒绝。
             * @default false
             */
            confirmed: boolean;
            path: string;
            /** @default false */
            recursive: boolean;
            /** @constant */
            type: "remove";
        };
        FileRequestRename: {
            from: string;
            to: string;
            /** @constant */
            type: "rename";
        };
        FileSubmit: {
            action: components["schemas"]["FileRequest"];
            /** @description 幂等键。与 actions 端点不同，本端点不校验其非空与长度。 */
            idempotency_key: string;
        };
        Finding: {
            /** @description 具体依据(中文,含实际值、期望值与阈值)。 */
            detail: string;
            /** @description 一句话标题(中文,如「集群成员数与期望不符」)。 */
            title: string;
            tone: components["schemas"]["Tone"];
        };
        /** @description Galera/wsrep 分区采集到的事实。 */
        GaleraState: {
            /** @description BF 中止累计值;null 表示未知。 */
            bf_aborts: number | null;
            /** @description 认证失败累计值;null 表示未知,0 表示无冲突。 */
            cert_failures: number | null;
            /** @description 本节点看到的集群成员数;null 表示未知。 */
            cluster_size: number | null;
            /** @description 集群状态 UUID,跨节点一致性判断用;null 表示未知。 */
            cluster_state_uuid: string | null;
            /** @description Primary / non-Primary / Disconnected。 */
            cluster_status: string;
            /** @description wsrep_connected;null 表示未知。 */
            connected: boolean | null;
            /** @description 是否处于 desync;null 表示未知。 */
            desync: boolean | null;
            /** @description 期望成员数,来自 Agent 本机巡检配置(默认 5,与告警规则一致);0 表示未知。 */
            expected_cluster_size: number;
            /** @description 流控暂停比例(0..1),超过 0.01 阈值才告警,恰好等于不算;null 表示未知。 */
            flow_control_paused: number | null;
            /** @description 流控暂停纳秒数;null 表示未知。 */
            flow_control_paused_ns: number | null;
            /** @description wsrep_incoming_addresses 拆开后的成员地址,可为空数组。 */
            incoming: string[];
            /** @description 最后提交的事务序号;null 表示未知。 */
            last_committed: number | null;
            /** @description wsrep 本地状态码,4 = Synced;null 表示未知。 */
            local_state: number | null;
            /** @description Synced / Donor/Desynced / Joining 等。 */
            local_state_comment: string;
            /** @description 本节点 UUID,重复即疑似克隆;null 表示未知。 */
            node_uuid: string | null;
            /** @description wsrep provider 版本;null 表示未知。 */
            provider_version: string | null;
            /** @description wsrep_ready;null 表示未知。 */
            ready: boolean | null;
            /** @description 接收队列长度;null 表示未知,不等于 0。告警阈值恒为 32。 */
            recv_queue: number | null;
            /** @description 发送队列长度;null 表示未知,不等于 0。 */
            send_queue: number | null;
            /** @description SST donor;null 表示未知。 */
            sst_donor: string | null;
        };
        HostInspectRequest: {
            /** @description 幂等键，用于去重同一个盘点动作；同节点同键不同参数返回 400。 */
            idempotency_key: string;
        };
        /** @description 单个网络接口的速率与累计错误。 */
        InterfaceRate: {
            /** @description 接口名。 */
            name: string;
            rx_bytes_per_second: number;
            /**
             * Format: int64
             * @description 接收错误累计。
             */
            rx_errors: number;
            tx_bytes_per_second: number;
            /**
             * Format: int64
             * @description 发送错误累计。
             */
            tx_errors: number;
        };
        LoginRequest: {
            /** @description 管理员密码，与初始化时写入的 Argon2 哈希比对。 */
            password: string;
        };
        /** @description 控制台指标总览条目：最近成功采样 + 当前采集状态。与上报帧分离，避免失败上报冲掉成功值。 */
        MetricsOverviewNode: {
            /** Format: int64 */
            clock_offset: number;
            /** @description 最近一次上报的失败原因；当前成功时为 null。 */
            error?: string | null;
            /**
             * Format: int64
             * @description 同 received_at，供界面统一读取新鲜度。
             */
            metrics_at: number | null;
            /** @enum {string} */
            metrics_status: "ok" | "collect_failed" | "unknown";
            node_id: string;
            /**
             * Format: int64
             * @description 最近一次成功采样接收时刻；无成功采样时为 null。
             */
            received_at: number | null;
            sample: components["schemas"]["MetricsSample"] | null;
        };
        /** @description 指标曲线上一个点。字段扁平、单位明确，便于直接绘图；缺失区间不插值不补零，直接缺点。 */
        MetricsPoint: {
            /**
             * Format: int64
             * @description 桶时刻（秒）。
             */
            at: number;
            /** @description CPU 使用率（百分比），桶内取平均。 */
            cpu_usage: number;
            disk_read_bytes_per_second: number;
            disk_write_bytes_per_second: number;
            load1: number;
            /**
             * Format: int64
             * @description 内存总量（字节）。
             */
            memory_total: number;
            /**
             * Format: int64
             * @description 已用内存（字节）。
             */
            memory_used: number;
            /** @description 所有网卡接收速率求和。 */
            net_rx_bytes_per_second: number;
            /** @description 所有网卡发送速率求和。 */
            net_tx_bytes_per_second: number;
            /**
             * Format: int64
             * @description 已用交换分区（字节）。
             */
            swap_used: number;
        };
        /** @description 一次主机指标采样。数值单位写在字段名中（bytes_per_second / 百分比），展示层无需再猜单位。 */
        MetricsSample: {
            /**
             * Format: int64
             * @description 采样时刻（秒，Agent 本地时钟）；主控据此估算时钟偏移。
             */
            at: number;
            /** @description 每核 CPU 使用率。 */
            cpu_per_core: number[];
            /** @description CPU 使用率（百分比）。 */
            cpu_usage: number;
            disk_read_bytes_per_second: number;
            disk_write_bytes_per_second: number;
            disks: components["schemas"]["DiskUsage"][];
            interfaces: components["schemas"]["InterfaceRate"][];
            load1: number;
            load5: number;
            load15: number;
            /**
             * Format: int64
             * @description 可用内存（字节）。
             */
            memory_available: number;
            /**
             * Format: int64
             * @description 内存总量（字节）。
             */
            memory_total: number;
            /**
             * Format: int64
             * @description 已用内存（字节）。
             */
            memory_used: number;
            /** @description 到各对端的延迟。探测频率低于采样频率，多数采样为空，稀疏探测时沿用上一次结果。 */
            peers?: components["schemas"]["PeerLatency"][];
            /**
             * Format: int64
             * @description 进程数。
             */
            processes: number;
            /**
             * Format: int64
             * @description 交换分区总量（字节）。
             */
            swap_total: number;
            /**
             * Format: int64
             * @description 已用交换分区（字节）。
             */
            swap_used: number;
            /**
             * Format: int64
             * @description TCP 连接数。
             */
            tcp_connections: number;
            /**
             * Format: int64
             * @description UDP 连接数。
             */
            udp_connections: number;
            /**
             * Format: int64
             * @description 系统运行时长（秒）。
             */
            uptime: number;
        };
        /** @description 单个指标层级的保留状态汇总。 */
        MetricsTierInfo: {
            /** @description 桶宽（秒）：raw=15、m5=300、h1=3600。 */
            bucket_seconds: number;
            /**
             * Format: int64
             * @description 最早一条记录的时刻（Unix 秒）；无记录时为 null。
             */
            oldest: number | null;
            /**
             * Format: int64
             * @description 该层级当前记录数。
             */
            records: number;
            /**
             * Format: int64
             * @description 保留时长（秒）：raw=7 天、m5=90 天、h1=365 天。
             */
            retention_seconds: number;
            /**
             * @description 层级名。
             * @enum {string}
             */
            tier: "raw" | "m5" | "h1";
        };
        Node: {
            /**
             * Format: int64
             * @description 该节点地址集合版本号。
             */
            address_version: number;
            capabilities?: components["schemas"]["NodeCapabilities"] | null;
            /** @description 展示用分组，缺省为 null。 */
            group?: string | null;
            /** @description 对未持分享令牌的访客隐藏，缺省为 false。 */
            hidden?: boolean;
            id: string;
            /** @description 主机盘点原始数据，深度结构由 Agent 决定（任意 JSON 对象，不约束字段）。 */
            inventory?: {
                [key: string]: unknown;
            } | null;
            /**
             * Format: int64
             * @description 最近在线时间（Unix 秒）。
             */
            last_seen: number;
            name: string;
            /** @description 覆盖网地址，可缺失或为 null。 */
            overlay_address?: string | null;
            public_addresses: string[];
            /** @description 公开备注，缺省为 null。 */
            public_remark?: string | null;
            /** @description 展示用地域，缺省为 null。 */
            region?: string | null;
            revoked: boolean;
            ssh_port: number;
            /** @description 展示用标签，缺省为空数组。 */
            tags?: string[];
            /** @description 展示排序权重，缺省为 0。 */
            weight?: number;
        };
        NodeCapabilities: {
            compose: boolean;
            /** @description 数据库只读巡检能力，缺省 false。 */
            database?: boolean;
            docker: boolean;
            /** @description Agent 实际运行方式，旧 Agent 缺省为 host。 */
            execution_mode?: components["schemas"]["AgentInstallMode"];
            /** @description 该节点可用的防火墙后端列表。 */
            firewall: string[];
            /** @description 主机盘点能力，缺省 false。 */
            hostinfo?: boolean;
            /** @description 指标采样能力，缺省 false。 */
            metrics?: boolean;
            os: string;
            /** @description 对端延迟探测能力，缺省 false。 */
            probe?: boolean;
            /** @description 存储集群能力，缺省 false。 */
            storage?: boolean;
            systemd: boolean;
        };
        NodeEntry: {
            /** @description 当前是否在线。运行时判定，不属于 Node 结构体。 */
            connected: boolean;
            node: components["schemas"]["Node"];
        };
        Ok: {
            /** @constant */
            ok: true;
        };
        Overview: {
            /** @description 跨节点一致性检查结论(集群 UUID 不一致、节点 UUID 重复、成员数不一致、多数派不足、无人按时备份)。 */
            checks: components["schemas"]["Finding"][];
            /** @description 期望集群成员数,取自各报告 expected_cluster_size 的第一个非 0 值;全部为 0 时此值为 0。 */
            expected_cluster_size: number;
            /**
             * Format: int64
             * @description 本视图生成时刻(Unix 秒)。
             */
            generated_at: number;
            /** @description 各节点结论,按名称排序,不含已撤销节点。 */
            nodes: components["schemas"]["DatabaseNodeVerdict"][];
            /** @description 一句话总结(已巡检台数、异常数、未知数等),中文。 */
            summary: string;
            tone: components["schemas"]["Tone"];
            /** @description 所有节点 unknown 字符串总数,用于一眼看出还有多少没看到。 */
            unknown_count: number;
        };
        PeerAddressSet: {
            /** @description 去重排序后的公网地址集合。 */
            addresses: string[];
            /** Format: int64 */
            version: number;
        };
        /** @description 到单个对端节点的延迟测量结果。 */
        PeerLatency: {
            address: string;
            /** @description 不可达原因。 */
            error?: string | null;
            /** @description 延迟（毫秒）。没测到时为 null 或缺省，绝不补 0（0 表示同机）。 */
            latency_ms?: number | null;
            /** @enum {string} */
            method: "icmp" | "tcp";
            node_id: string;
            /**
             * Format: int64
             * @description 测量时刻，可早于采样时刻。
             */
            probed_at?: number;
            /** @description 是否可达。 */
            reachable: boolean;
        };
        /** @description 存储部署计划，同时是「将要发生什么」的说明和执行的凭据。 */
        Plan: {
            /** @description 集群名。 */
            cluster: string;
            /** @description 已执行完成的节点 id 列表，缺省空数组。 */
            completed?: string[];
            /**
             * Format: int64
             * @description 计划创建时间（unix 秒）。
             */
            created_at: number;
            /** @description 是否包含不可逆步骤（本模块恒为 true）。 */
            destructive: boolean;
            /**
             * Format: int64
             * @description 有效期（unix 秒），生成时 = now()+300（5 分钟）；过期计划不能执行。
             */
            expires: number;
            /** @description 失败节点与原因，元素为 [node_id, reason] 二元数组；非空即视为计划失败，阻断后续节点。缺省空数组。 */
            failed?: string[][];
            /** @description 预检指纹，候选集合变化即失效，必须重新生成计划。 */
            fingerprint: string;
            /** @description 计划 id。 */
            id: string;
            /** @description 镜像引用，形如 pgsty/silo:<image_tag>。 */
            image: string;
            /** @description 不可逆步骤告知，界面必须原文展示。 */
            irreversible_notice: string;
            /** @description 计划内各节点步骤。 */
            nodes: components["schemas"]["PlannedNode"][];
        };
        PlannedNode: {
            /** @description 即将被清空的设备及计划时的容量。 */
            devices: components["schemas"]["StorageDeviceTarget"][];
            /** @description 该节点执行完成后对外提供的挂载点。 */
            mounts: string[];
            /** @description 节点名。 */
            name: string;
            /** @description 节点编号。 */
            node_id: string;
        };
        Point: {
            /**
             * Format: int64
             * @description 时间戳（Unix 秒），取所在步长桶内**首条采样**自己的时间戳，不与桶边界对齐。
             */
            at: number;
            /** @description CPU 使用百分比。 */
            cpu_usage: number;
            /** @description 磁盘读速率（字节/秒）。 */
            disk_read_bytes_per_second: number;
            /** @description 磁盘写速率（字节/秒）。 */
            disk_write_bytes_per_second: number;
            /** @description 1 分钟平均负载。 */
            load1: number;
            /**
             * Format: int64
             * @description 内存总量字节。
             */
            memory_total: number;
            /**
             * Format: int64
             * @description 已用内存字节。
             */
            memory_used: number;
            /** @description 入站速率（字节/秒），各网卡求和。 */
            net_rx_bytes_per_second: number;
            /** @description 出站速率（字节/秒），各网卡求和。 */
            net_tx_bytes_per_second: number;
            /**
             * Format: int64
             * @description 已用交换分区字节。
             */
            swap_used: number;
        };
        ProbeTarget: {
            /** @description 被探测地址，优先使用覆盖网地址。 */
            address: string;
            node_id: string;
            /** @description 给了端口走 TCP 握手计时；为空时走 ICMP。 */
            port?: number | null;
        };
        ProbeTargetSet: {
            targets?: components["schemas"]["ProbeTarget"][];
            /** Format: int64 */
            version: number;
        };
        ProxySqlDigest: {
            /**
             * Format: int64
             * @description 执行次数。
             */
            count_star: number;
            /** @description 摘要哈希,可作跨节点比对的稳定标识。 */
            digest: string;
            /** @description hostgroup 编号。 */
            hostgroup: number;
            /**
             * Format: int64
             * @description 最大耗时。
             */
            max_time: number;
            /** @description 库名。 */
            schemaname: string;
            /**
             * Format: int64
             * @description 总耗时。
             */
            sum_time: number;
        };
        /** @description 连接池计数行,来自 stats_mysql_connection_pool;各计数 null 表示无该值或未知,不补 0。 */
        ProxySqlPool: {
            /**
             * Format: int64
             * @description 接收字节数;null 表示未知。
             */
            bytes_recv: number | null;
            /**
             * Format: int64
             * @description 发送字节数;null 表示未知。
             */
            bytes_sent: number | null;
            /** @description 失败连接数;null 表示未知。 */
            conn_err: number | null;
            /** @description 空闲连接数;null 表示未知。 */
            conn_free: number | null;
            /** @description 成功连接数;null 表示未知。 */
            conn_ok: number | null;
            /** @description 已用连接数;null 表示未知。 */
            conn_used: number | null;
            /** @description hostgroup 编号。 */
            hostgroup: number;
            /** @description 延迟(微秒);null 表示未知。 */
            latency_us: number | null;
            /** @description 查询数;null 表示未知。 */
            queries: number | null;
            /** @description 后端主机。 */
            srv_host: string;
            /** @description 端口。 */
            srv_port: number;
        };
        ProxySqlServer: {
            /** @description 空闲连接数;null 表示未知。 */
            conn_free: number | null;
            /** @description 已用连接数;null 表示未知。 */
            conn_used: number | null;
            /** @description hostgroup 编号。 */
            hostgroup_id: number;
            /** @description 后端主机。 */
            hostname: string;
            /** @description 来自 stats_mysql_connection_pool.Latency_us;无该行时为 null,不补 0。 */
            latency_us: number | null;
            /** @description 最大连接数。 */
            max_connections: number;
            /** @description 端口。 */
            port: number;
            /** @description 查询数;null 表示未知。 */
            queries: number | null;
            /** @description ONLINE / SHUNNED / OFFLINE_SOFT 等。 */
            status: string;
            /** @description 权重。 */
            weight: number;
        };
        /** @description ProxySQL 分区采集到的事实。 */
        ProxySqlState: {
            /** @description 备用 writer 组(hostgroup 20)在线后端数;null 为未知,判定下限为 4。 */
            backup_writer_online: number | null;
            /** @description 按总耗时排序的前若干条摘要,可为空数组。 */
            digests: components["schemas"]["ProxySqlDigest"][];
            /** @description 连接池计数列表,可为空数组。 */
            pools: components["schemas"]["ProxySqlPool"][];
            /** @description 后端节点列表,可为空数组。 */
            servers: components["schemas"]["ProxySqlServer"][];
            /**
             * Format: int64
             * @description 运行秒数;null 表示未知。
             */
            uptime: number | null;
            /** @description ProxySQL 版本;null 表示未知。 */
            version: string | null;
            /** @description writer 组(hostgroup 10)在线后端数;null 为未知,判定阈值恒为 1。 */
            writer_online: number | null;
        };
        PublicNode: {
            /** @description CPU 使用百分比，无采样为 null（不是 0）。 */
            cpu_usage: number | null;
            /**
             * Format: int64
             * @description 磁盘总容量字节，无采样为 null。
             */
            disk_total: number | null;
            /**
             * Format: int64
             * @description 磁盘总已用字节（累加各挂载点），无采样为 null。
             */
            disk_used: number | null;
            /** @description 分组，可缺省可 null。 */
            group: string | null;
            /** @description 节点编号。 */
            id: string;
            /** @description 1 分钟平均负载，无采样为 null。 */
            load1: number | null;
            /**
             * Format: int64
             * @description 内存总量字节，无采样为 null。
             */
            memory_total: number | null;
            /**
             * Format: int64
             * @description 已用内存字节，无采样为 null。
             */
            memory_used: number | null;
            /**
             * Format: int64
             * @description 最近一次成功采样的接收时刻（Unix 秒）；没有成功采样时为 null。采集失败上报不会刷新它。
             */
            metrics_at: number | null;
            /**
             * @description 当前采集状态：ok=最近上报成功；collect_failed=最近上报失败（指标字段仍是上次成功值）；
             *     unknown=从未上报。与 online 互不替代。
             * @enum {string}
             */
            metrics_status: "ok" | "collect_failed" | "unknown";
            /** @description 节点显示名。 */
            name: string;
            /**
             * Format: int64
             * @description 入站速率，各网卡求和后取整，无采样为 null。
             */
            net_rx_bytes_per_second: number | null;
            /**
             * Format: int64
             * @description 出站速率，各网卡求和后取整，无采样为 null。
             */
            net_tx_bytes_per_second: number | null;
            /** @description 是否在线（存在通道连接）。 */
            online: boolean;
            /** @description 区域，可缺省可 null。 */
            region: string | null;
            /** @description 公开备注（内部字段 public_remark 的公开别名），可缺省可 null。 */
            remark: string | null;
            /** @description 标签列表，缺省为空数组。 */
            tags: string[];
            /**
             * Format: int64
             * @description 运行时长秒，无采样为 null。
             */
            uptime: number | null;
            /**
             * Format: int64
             * @description 排序权重。
             */
            weight: number;
        };
        Registration: {
            /** @description Agent 安装方式，缺省为宿主机 systemd。 */
            install_mode?: components["schemas"]["AgentInstallMode"];
            /** @description 节点名称，非空且不超过 100 字符。 */
            name: string;
            /** @description 覆盖网地址，须为合法 IP。 */
            overlay_address?: string | null;
            /** @description 可路由的公网地址，缺省为空数组。 */
            public_addresses?: string[];
            /** @description SSH 端口，缺省 22。 */
            ssh_port?: number;
        };
        Rejected: {
            /** @description 设备名。 */
            device: string;
            /** @description 拒绝原因。 */
            reason: string;
        };
        RenewRequest: {
            csr: string;
        };
        RenewResponse: {
            /** @description 续期后的客户端证书 PEM，有效期 90 天。 */
            certificate: string;
        };
        Report: {
            /** @description 一致性检查项。 */
            checks: components["schemas"]["Check"][];
            /**
             * Format: int64
             * @description 报告生成时间（unix 秒）。
             */
            generated_at: number;
            /** @description 逐节点结论。 */
            nodes: components["schemas"]["StorageNodeVerdict"][];
            /** @description 总体是否可直接进入部署计划。 */
            ready: boolean;
            /** @description 面向运维的总结说明。 */
            summary: string;
            /** @description 推荐拓扑；条件不满足时为 null。 */
            topology?: components["schemas"]["Topology"] | null;
        };
        Session: {
            /** @description 本会话的 CSRF 令牌，后续非 GET/HEAD 请求要写回 x-csrf-token 头。 */
            csrf: string;
            /**
             * Format: int64
             * @description 会话过期时间（Unix 秒）。
             */
            expires: number;
        };
        ShareNodes: {
            /** @description 可见节点列表，按权重降序、再按名称升序；只含公开白名单字段。 */
            nodes: components["schemas"]["PublicNode"][];
        };
        ShareRecent: {
            /** @description 该节点最近一次采样折算的公开结构；无采样的指标字段为 null 而不是 0。 */
            node: components["schemas"]["PublicNode"];
        };
        ShareRecords: {
            /** @description 回显请求的节点编号。 */
            node: string;
            /** @description 降采样后的曲线点，点数上限 2000；缺失区间自然断开，不插值不补零。 */
            points: components["schemas"]["Point"][];
            /**
             * Format: int64
             * @description 实际使用的步长（clamp 到 [60, 86400] 之后）。
             */
            step: number;
            /**
             * @description 选用的层级：step<300 为 raw、step<3600 为 m5、其余为 h1。
             * @enum {string}
             */
            tier: "raw" | "m5" | "h1";
        };
        ShareSettings: {
            /** @description 分享页整体开关。 */
            enabled: boolean;
            site: components["schemas"]["SitePublic"];
        };
        /** @description 更新分享设置的请求体。拒绝未知字段（serde deny_unknown_fields），与 additionalProperties: false 一致。 */
        ShareSettingsInput: {
            /** @description 分享页整体开关。 */
            enabled: boolean;
            /** @description 站点公开信息;其下 name/description/footer 三者均必填。 */
            site: components["schemas"]["SitePublic"];
        };
        ShareSummary: {
            /**
             * Format: int64
             * @description 响应生成时的 Unix 秒时间戳。
             */
            generated_at: number;
            /**
             * Format: int64
             * @description 令牌可见（未撤销、未隐藏且在令牌范围内）的节点数。
             */
            node_count: number;
            /**
             * Format: int64
             * @description 其中在线的节点数（存在活跃通道连接）。
             */
            online_count: number;
            site: components["schemas"]["SitePublic"];
        };
        /** @description 机器接口的汇总，刻意不下发站点信息（site），只给交集内的计数与生成时刻。 */
        ShareSummaryBase: {
            /**
             * Format: int64
             * @description 响应生成时的 Unix 秒时间戳。
             */
            generated_at: number;
            /**
             * Format: int64
             * @description 令牌范围与 API Key 节点范围交集后的可见节点数。
             */
            node_count: number;
            /**
             * Format: int64
             * @description 其中在线的节点数（存在活跃通道连接）。
             */
            online_count: number;
        };
        ShareToken: {
            /**
             * Format: int64
             * @description 创建时间,Unix 秒。
             */
            created_at: number;
            /** @description sha256(明文令牌) 十六进制摘要,门禁常量时间查找用;明文永不落库、不再出现在任何响应中。 */
            digest: string;
            /**
             * Format: int64
             * @description 到期时间,Unix 秒;键恒存在,null 表示长期有效。
             */
            expires_at: number | null;
            /** @description 令牌记录 id。 */
            id: string;
            /** @description 分享名称。 */
            label: string;
            /** @description 限定可见节点 id 列表;null 表示全部未隐藏节点。 */
            nodes: string[] | null;
            /** @description 是否已撤销;撤销后门禁立即失效,记录仍留在列表。 */
            revoked: boolean;
        };
        ShareTokenCreated: {
            record: components["schemas"]["ShareToken"];
            /** @description 明文令牌,仅此一次返回,之后无法再取回。 */
            token: string;
        };
        /** @description 创建分享令牌的请求体。拒绝未知字段（serde deny_unknown_fields），与 additionalProperties: false 一致。 */
        ShareTokenInput: {
            /**
             * Format: int64
             * @description 有效小时数,范围 1..=8760(24*365);省略或 null 表示长期有效。
             */
            expires_hours?: number | null;
            /** @description 分享名称,trim 后非空且不超过 60 字符,否则 400。 */
            label: string;
            /** @description 限定可见节点 id 列表;省略或 null 表示全部未隐藏节点。 */
            nodes?: string[] | null;
        };
        ShareTokenList: {
            /** @description 按 created_at 降序;只含摘要与元数据,绝不包含明文令牌(明文仅创建响应一次)。 */
            tokens: components["schemas"]["ShareToken"][];
        };
        SitePublic: {
            /** @description 站点描述,缺省 "节点状态";写入时超过 200 字符被拒。 */
            description: string;
            /** @description 页脚自定义文字,不允许 HTML,缺省为空串;写入时超过 200 字符被拒。 */
            footer: string;
            /** @description 站点名称,缺省 "opsd";写入时超过 60 字符被拒。 */
            name: string;
        };
        /** @enum {string} */
        StackOperation: "start" | "stop" | "remove";
        StorageDeviceTarget: {
            /**
             * Format: int64
             * @description 预期容量。与实测不符时预检拒绝，避免格式化错盘。
             */
            expected_size: number;
            /** @description 挂载点，例如 /data/disk1。 */
            mount: string;
            /** @description 块设备路径。 */
            path: string;
        };
        StorageNodeVerdict: {
            /** @description 候选盘。 */
            candidates: components["schemas"]["Candidate"][];
            /**
             * Format: int64
             * @description 盘点时间（unix 秒），未采集为 null。
             */
            collected_at?: number | null;
            /** @description 采集缺口。 */
            gaps: string[];
            /**
             * Format: int64
             * @description 该节点最快物理链路速率（Mbps），未采集为 null。
             */
            link_mbps?: number | null;
            /** @description 节点名。 */
            name: string;
            /** @description 节点编号。 */
            node_id: string;
            /** @description 是否在线。 */
            online: boolean;
            /** @description 逐条结论原因，界面直接展示。 */
            reasons: string[];
            /** @description 被拒盘及原因，便于运维核对。 */
            rejected: components["schemas"]["Rejected"][];
            suitability: components["schemas"]["Suitability"];
        };
        StoragePlanApplyRequest: {
            /**
             * @description 破坏性确认字段（名称逐字为 acknowledge_destructive）；计划为破坏性时必须显式 true 才能执行，缺省 false 即拒绝。
             * @default false
             */
            acknowledge_destructive: boolean;
            /** @description 要执行的计划 id。 */
            plan_id: string;
        };
        StoragePlanApplyResult: {
            /** @description 已完整执行完成的节点 id 列表。 */
            completed: string[];
            /** @description 每个节点每个已下发步骤的记录。 */
            dispatched: components["schemas"]["DispatchRecord"][];
            /** @description 本次执行的计划 id。 */
            plan_id: string;
        };
        StorageTaskAccepted: {
            /** @description 动作标签，由各操作固定：状态 / 桶 / 用户。 */
            label: string;
            /** @description 被选中执行任务的节点。 */
            node: string;
            /** @description 下发给集群节点的任务 id。 */
            task_id: string;
        };
        Submit: {
            action: components["schemas"]["Action"];
            /** @description 幂等键，非空且不超过 128 字符。 */
            idempotency_key: string;
        };
        /**
         * @description 结论等级：可直接进入部署计划 / 有条件可用 / 硬性阻断 / 尚未采集（不算不适合）。
         * @enum {string}
         */
        Suitability: "suitable" | "needs_work" | "unsuitable" | "pending";
        TaskAccepted: {
            /** @description 任务 UUID。相同节点、相同幂等键、相同参数摘要的重放会返回同一个 id。 */
            task_id: string;
        };
        TaskEnvelope: {
            action: components["schemas"]["Action"];
            /** Format: int64 */
            created_at: number;
            /** @description 参数摘要（SHA-256）。相同节点与幂等键下摘要不同即视为冲突。 */
            digest: string;
            /** @description 失败信息，可为 null。 */
            error?: string | null;
            id: string;
            /** @description 提交时的幂等键，与 node_id 组成唯一约束。 */
            key: string;
            /** @description 目标节点编号。 */
            node_id: string;
            /** @description 任务结果，任意 JSON 对象，可为 null（形状由具体操作决定）。 */
            result?: {
                [key: string]: unknown;
            } | null;
            /**
             * Format: int64
             * @description 进度序号。新建为 0，Agent 每次上报递增。
             */
            sequence?: number;
            status: components["schemas"]["TaskStatus"];
            /** Format: int64 */
            updated_at: number;
        };
        TaskEvent: {
            id: string;
            /** @description 有错误时取错误文本，否则为状态描述。 */
            message: string;
            /**
             * Format: int64
             * @description 事件序号，等于该次上报后的任务 sequence。
             */
            sequence: number;
            status: components["schemas"]["TaskStatus"];
            task_id: string;
            /** Format: int64 */
            time: number;
        };
        /**
         * @description 任务状态。终态为 succeeded / failed / uncertain / rolled_back / blocked / cancelled。
         * @enum {string}
         */
        TaskStatus: "pending" | "accepted" | "running" | "validating" | "succeeded" | "failed" | "uncertain" | "rollback_pending" | "rolled_back" | "blocked" | "cancelled";
        ThemeActivateRequest: {
            /** @description 主题配置键值对，键为配置项 key；省略时用声明默认值补齐。 */
            settings?: {
                [key: string]: unknown;
            };
            /** @description 要启用的主题 short；default（大小写不敏感）表示回到内置主题。 */
            short: string;
            /**
             * @description 目标界面，大小写敏感。
             * @enum {string}
             */
            surface: "console" | "share";
        };
        ThemeActivateResult: {
            /** @description 实际生效的设置（含默认值补齐）；回落到内置主题时响应不含此键。 */
            settings?: {
                [key: string]: unknown;
            };
            /** @description 已启用的主题 short；回落到内置主题时同样为 default。 */
            short: string;
        };
        ThemeActive: {
            /** @description 控制台界面当前启用主题（describe 输出）。 */
            console: components["schemas"]["ThemeDescribe"];
            /** @description 分享页界面当前启用主题（describe 输出）。 */
            share: components["schemas"]["ThemeDescribe"];
        };
        /** @description 托管配置声明；首版只接受 managed，raw 不做。 */
        ThemeConfiguration: {
            /** @description 配置项声明列表，缺省为空数组。 */
            data?: components["schemas"]["ThemeField"][];
            /**
             * @description 配置托管方式，默认为 managed。
             * @enum {string}
             */
            type?: "managed";
        };
        /** @description 当前启用主题的 describe 输出；未启用时回落到内置主题形态。 */
        ThemeDescribe: {
            /** @description 配置项声明；未启用时为空数组。 */
            fields: components["schemas"]["ThemeField"][];
            /** @description 主题名；内置主题为 {"zh-CN":"内置主题"}。 */
            name: components["schemas"]["ThemeLocalized"];
            /** @description 主题设置（含默认值补齐）。分享页主题的设置公开可读，不得放入密钥；未启用时为空对象。 */
            settings: {
                [key: string]: unknown;
            };
            /** @description 启用的主题 short；未启用时为 default。 */
            short: string;
            /** @description 令牌值，未启用时为 null。 */
            tokens: components["schemas"]["ThemeTokens"] | null;
        };
        /** @description 托管配置项声明。 */
        ThemeField: {
            /** @description 默认值，可为任意 JSON 值。 */
            default?: unknown;
            /** @description 帮助文本，可为 null。 */
            help?: components["schemas"]["ThemeLocalized"] | null;
            /** @description 配置项标识，非空且不超过 48 字符。 */
            key: string;
            /** @description 配置项显示名。 */
            name: components["schemas"]["ThemeLocalized"];
            /** @description 仅 select 使用：逗号分隔的选项列表。 */
            options?: string | null;
            /**
             * @description 配置项类型；select 必须给出 options。
             * @enum {string}
             */
            type: "switch" | "select" | "number" | "string" | "text";
        };
        ThemeInstallConsoleResult: {
            /** @description 已安装的主题 short；同 short 覆盖既有记录。 */
            short: string;
        };
        /** @description 已安装主题的索引条目。 */
        ThemeInstalled: {
            /** @description 多语言作者信息，可为 null。 */
            author?: components["schemas"]["ThemeLocalized"] | null;
            /** @description 多语言描述，可为 null。 */
            description?: components["schemas"]["ThemeLocalized"] | null;
            /** @description 内容摘要（sha256 hex）。 */
            digest: string;
            /** @description 配置项声明，取自 manifest.configuration.data，无则空数组。 */
            fields: components["schemas"]["ThemeField"][];
            /** @description 包级主题是否已解压出资源目录。 */
            has_assets: boolean;
            /**
             * Format: int64
             * @description 安装时间（Unix 秒）。
             */
            installed_at: number;
            /** @description 多语言名称。 */
            name: components["schemas"]["ThemeLocalized"];
            /** @description 主题唯一标识。 */
            short: string;
            /** @description 主题覆盖的界面。 */
            surfaces: components["schemas"]["ThemeSurface"][];
            /** @description 版本号，可为 null。 */
            version?: string | null;
        };
        ThemeInstallShareResult: {
            /** @description 主控对上传原始字节流计算的 sha256（hex），不信任调用方提供的校验值。 */
            digest: string;
            /** @description 安装的主题 short；同 short 覆盖旧记录与资源目录。 */
            short: string;
        };
        ThemeList: {
            /** @description 已安装主题列表，按 short 字典序排列。 */
            themes: components["schemas"]["ThemeInstalled"][];
        };
        /** @description 多语言文本，二选一：普通字符串，或语言代码到文本的映射（如 {"zh-CN":"...","en":"..."}）。 */
        ThemeLocalized: string | {
            [key: string]: string;
        };
        /** @description 主题清单。控制台主题以此 JSON 提交；分享页主题包内的 theme.json 同构。 */
        ThemeManifest: {
            /** @description 多语言作者信息，可为 null。 */
            author?: components["schemas"]["ThemeLocalized"] | null;
            /** @description 托管配置项声明，仅控制台主题使用。 */
            configuration?: components["schemas"]["ThemeConfiguration"] | null;
            /** @description 多语言描述，可为 null。 */
            description?: components["schemas"]["ThemeLocalized"] | null;
            /** @description 多语言名称。 */
            name: components["schemas"]["ThemeLocalized"];
            /** @description 预览图，可为 null。 */
            preview?: string | null;
            /** @description 唯一标识：只含大小写字母/数字/下划线/连字符，长度 1-48，不得为 default（大小写不敏感）。 */
            short: string;
            /** @description 界面枚举，必须非空。 */
            surfaces: components["schemas"]["ThemeSurface"][];
            /** @description 令牌覆盖，仅控制台主题使用；声明了 console 则必须提供。 */
            tokens?: components["schemas"]["ThemeTokens"] | null;
            /** @description 版本号，可为 null。 */
            version?: string | null;
        };
        /**
         * @description 主题覆盖的界面。
         * @enum {string}
         */
        ThemeSurface: "share" | "console";
        /** @description 控制台主题的令牌覆盖，键是 style.css 中的自定义属性名。 */
        ThemeTokens: {
            /** @description 深色令牌，同上。 */
            dark?: {
                [key: string]: string;
            };
            /** @description 浅色令牌：CSS 自定义属性名到值的映射。 */
            light?: {
                [key: string]: string;
            };
        };
        /**
         * @description 结论严重程度。只描述已确定的问题:未知不参与排序,无任何发现时为 ok。
         * @enum {string}
         */
        Tone: "ok" | "warning" | "critical";
        Topology: {
            /**
             * Format: int64
             * @description 最小盘容量字节，容量按最小盘计算。
             */
            drive_size: number;
            /** @description 每节点盘数。 */
            drives_per_node: number;
            /** @description 节点数。 */
            nodes: number;
            /** @description 纠删码校验盘数。 */
            parity: number;
            /** @description 校验说明，例如「共 N 块盘，校验 P 块；容量按最小盘 X 计算」。 */
            parity_note: string;
            /**
             * Format: int64
             * @description 原始裸容量。
             */
            raw_capacity: number;
            /**
             * Format: int64
             * @description 扣除校验后的可用容量。
             */
            usable_capacity: number;
        };
        UserRequest: {
            /** @description 集群定义名称。 */
            cluster: string;
            /**
             * @description 策略名，缺省 readwrite。
             * @default readwrite
             */
            policy: string;
            /**
             * @description true 时删除用户，缺省 false。
             * @default false
             */
            remove: boolean;
            /** @description 用户密钥。 */
            secret: string;
            /** @description 访问用户名。 */
            user: string;
        };
        VerifyRequest: {
            /** @description 无 witness 时是否必须独立验证，缺省 false。 */
            required?: boolean;
            /** @description 指定验证节点 id；缺失时自动挑选一个非本节点的在线节点。 */
            witness?: string | null;
        };
        VerifyResponse: {
            /**
             * @description 恒为 true，表示结果来自新建连接而非复用连接。
             * @constant
             */
            fresh_tls: true;
            /** @constant */
            ok: true;
        };
    };
    responses: {
        /** @description 业务 {"error"} 与框架纯文本 400 的双形态。 */
        BadRequest: {
            headers: {
                [name: string]: unknown;
            };
            content: {
                "application/json": components["schemas"]["Error"];
                "text/plain": string;
            };
        };
        /** @description Origin 或 CSRF 校验失败。注意正文是**纯文本**，不是 JSON。 */
        OriginOrCsrfRejected: {
            headers: {
                [name: string]: unknown;
            };
            content: {
                "text/plain": "请求来源或 CSRF 校验失败";
            };
        };
        /** @description 请求体超过控制台面 1MB 上限。 */
        PayloadTooLarge: {
            headers: {
                [name: string]: unknown;
            };
            content?: never;
        };
        /** @description 未登录。正文 {"error":"请先登录"}。 */
        Unauthorized: {
            headers: {
                [name: string]: unknown;
            };
            content: {
                "application/json": components["schemas"]["Error"];
            };
        };
        /** @description JSON 合法但无法反序列化（字段缺失/类型错/未知字段）。axum 0.8 返回 422，正文纯文本。 */
        UnprocessableEntity: {
            headers: {
                [name: string]: unknown;
            };
            content: {
                "text/plain": string;
            };
        };
        /** @description Content-Type 不是 application/json。正文是**纯文本**。 */
        UnsupportedMediaType: {
            headers: {
                [name: string]: unknown;
            };
            content: {
                "text/plain": string;
            };
        };
    };
    parameters: {
        /** @description 节点编号。所有资源一律以 node_id + resource_id 定位。 */
        NodeId: string;
        /** @description 任务 UUID。 */
        TaskId: string;
    };
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export type operations = Record<string, never>;
